use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use clap::Parser;
use futures_lite::StreamExt;
use regex::Regex;
use serde::Deserialize;
use sqlx::{MySql, Pool};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};
use trillium::Conn;

const FACE_SIZE: usize = 8;
const FACE_BYTES: usize = FACE_SIZE * FACE_SIZE * 4;
const ALLOWED_SCALES: [u32; 5] = [1, 2, 4, 8, 16];
static PLACEHOLDER: &[u8] = include_bytes!("placeholder.png");

#[derive(Deserialize)]
struct TextureMeta {
    url: String,
}
#[derive(Deserialize)]
struct TextureListMeta {
    #[serde(alias = "SKIN")]
    skin: TextureMeta,
}
#[derive(Deserialize)]
struct AvatarMeta {
    textures: TextureListMeta,
}

#[derive(Deserialize)]
struct MojangProfile {
    id: String,
}

#[derive(Deserialize)]
struct MojangProperty {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct MojangSession {
    properties: Vec<MojangProperty>,
}

#[derive(Deserialize)]
struct MojangTextures {
    textures: TextureListMeta,
}

#[derive(Parser, Debug)]
#[command(version, about = "Stream Minecraft skin faces as PNGs")]
struct Cli {
    #[arg(long, env = "RENSKIN_BIND", default_value = "127.0.0.1:3727")]
    bind: String,
    #[arg(long, env = "RENSKIN_CACHE_DIR", default_value = ".cache")]
    cache_root: PathBuf,
    /// Optional SkinSystem MySQL URL. Failed startup connection only disables this resolver.
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,
    /// Try Ely.by's SkinSystem before the default Mojang resolver.
    #[arg(long, env = "RENSKIN_ELY_BY")]
    ely_by: bool,
    #[arg(
        long,
        env = "RENSKIN_ELY_BY_BASE",
        default_value = "http://skinsystem.ely.by"
    )]
    ely_by_base: String,
    #[arg(long, env = "RENSKIN_CACHE_KEEP_HOURS", default_value_t = 24)]
    cache_keep_hours: u64,
}

#[derive(Default)]
struct CacheCounters {
    render_hit: AtomicU64,
    raw_hit: AtomicU64,
    scale_hit: AtomicU64,
    remote_fetch: AtomicU64,
    failed: AtomicU64,
}

struct State {
    username_regex: Regex,
    pool: Option<Pool<MySql>>,
    http: trillium_client::Client,
    user_agent: &'static str,
    cache_root: PathBuf,
    ely_by: Option<String>,
    counters: CacheCounters,
}
impl State {
    fn cache_path(&self, kind: &str, name: &str) -> PathBuf {
        self.cache_root.join(kind).join(name)
    }
}

async fn query(pool: &Pool<MySql>, nick: &str) -> Result<AvatarMeta> {
    let sql = "SELECT CONVERT(FROM_BASE64(sk.Value) USING UTF8) as data FROM sr_cache AS pl JOIN sr_player_skins AS sk ON pl.`uuid` = sk.`uuid` WHERE LOWER(pl.`name`) = LOWER(?) OR LOWER(sk.last_known_name) = LOWER(?) LIMIT 1";
    let (json,): (String,) = sqlx::query_as(sql)
        .bind(nick)
        .bind(nick)
        .fetch_one(pool)
        .await?;
    Ok(serde_json::from_str(&json)?)
}

async fn get_bytes(state: &State, url: &str) -> Result<Vec<u8>> {
    let mut response = state
        .http
        .get(url)
        .with_request_header("User-Agent", state.user_agent)
        .await?;
    let status = response.status().context("skin server sent no status")?;
    if !status.is_success() {
        bail!("skin server returned {status}");
    }
    Ok(response.response_body().read_bytes().await?)
}

async fn fetch_raw(state: &State, url: &str, path: &Path) -> Result<Vec<u8>> {
    let bytes = get_bytes(state, url).await?;
    async_fs::write(path, &bytes).await?;
    state.counters.remote_fetch.fetch_add(1, Ordering::Relaxed);
    Ok(bytes)
}

async fn resolve_mojang(state: &State, name: &str) -> Result<String> {
    let profile: MojangProfile = serde_json::from_slice(
        &get_bytes(
            state,
            &format!("https://api.mojang.com/users/profiles/minecraft/{name}"),
        )
        .await?,
    )?;
    let session: MojangSession = serde_json::from_slice(
        &get_bytes(
            state,
            &format!(
                "https://sessionserver.mojang.com/session/minecraft/profile/{}",
                profile.id
            ),
        )
        .await?,
    )?;
    let property = session
        .properties
        .iter()
        .find(|property| property.name == "textures")
        .context("Mojang profile has no textures property")?;
    let textures: MojangTextures = serde_json::from_slice(&STANDARD.decode(&property.value)?)?;
    Ok(textures.textures.skin.url)
}

/// Decode only rows 0..=15. Rows 8..=15 yield x=8..15 (face) and x=40..47
/// (hat); abandoning the reader here avoids a full decoded skin allocation.
fn decode_face_rows(png_bytes: &[u8]) -> Result<[u8; FACE_BYTES]> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    if info.width < 48 || info.height < 16 || info.interlaced {
        bail!("expected non-interlaced skin at least 48x16 pixels");
    }
    let mut base = [0_u8; FACE_BYTES];
    let mut hat = [0_u8; FACE_BYTES];
    for y in 0..16 {
        let row = reader.next_row()?.context("skin ended before row 16")?;
        if y >= 8 {
            let dst = (y - 8) * 32;
            let data = row.data();
            base[dst..dst + 32].copy_from_slice(&data[32..64]);
            hat[dst..dst + 32].copy_from_slice(&data[160..192]);
        }
    }
    Ok(composite_face(base, hat))
}

fn div_255_round(value: u32) -> u32 {
    (value + 127) / 255
}

/// Correct straight-alpha source-over, kept as the portable reference.
fn composite_pixel(dst: &mut [u8], src: &[u8]) {
    let sa = u32::from(src[3]);
    let da = u32::from(dst[3]);
    let out_a = sa + div_255_round(da * (255 - sa));
    if out_a == 0 {
        dst.fill(0);
        return;
    }
    for channel in 0..3 {
        let premultiplied =
            u32::from(src[channel]) * sa + div_255_round(u32::from(dst[channel]) * da * (255 - sa));
        dst[channel] = ((premultiplied + out_a / 2) / out_a) as u8;
    }
    dst[3] = out_a as u8;
}

fn composite_face_scalar(mut base: [u8; FACE_BYTES], hat: [u8; FACE_BYTES]) -> [u8; FACE_BYTES] {
    for (dst, src) in base.chunks_exact_mut(4).zip(hat.chunks_exact(4)) {
        composite_pixel(dst, src);
    }
    base
}

#[cfg(all(feature = "avx2", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn composite_face_avx2(base: [u8; FACE_BYTES], hat: [u8; FACE_BYTES]) -> [u8; FACE_BYTES] {
    use std::arch::x86_64::{__m256i, _mm256_loadu_si256, _mm256_storeu_si256};
    let mut base_rows = [0_u8; FACE_BYTES];
    let mut hat_rows = [0_u8; FACE_BYTES];
    for row in 0..FACE_SIZE {
        let offset = row * 32;
        unsafe {
            _mm256_storeu_si256(
                base_rows.as_mut_ptr().add(offset).cast::<__m256i>(),
                _mm256_loadu_si256(base.as_ptr().add(offset).cast::<__m256i>()),
            );
            _mm256_storeu_si256(
                hat_rows.as_mut_ptr().add(offset).cast::<__m256i>(),
                _mm256_loadu_si256(hat.as_ptr().add(offset).cast::<__m256i>()),
            );
        }
    }
    composite_face_scalar(base_rows, hat_rows)
}

fn composite_face(base: [u8; FACE_BYTES], hat: [u8; FACE_BYTES]) -> [u8; FACE_BYTES] {
    #[cfg(all(feature = "avx2", target_arch = "x86_64"))]
    if std::is_x86_feature_detected!("avx2") {
        return unsafe { composite_face_avx2(base, hat) };
    }
    composite_face_scalar(base, hat)
}

fn encode_png(rows: &[u8], width: u32, height: u32, filter: png::Filter) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(filter);
        encoder.write_header()?.write_image_data(rows)?;
    }
    Ok(output.into_inner())
}

/// Fixed integer replication for the only accepted factors; no scaled image type.
fn scale_and_encode(face: &[u8; FACE_BYTES], scale: u32, filter: png::Filter) -> Result<Vec<u8>> {
    let scale = usize::try_from(scale)?;
    let width = FACE_SIZE * scale;
    let mut rows = Vec::with_capacity(width * width * 4);
    for source_row in face.chunks_exact(32) {
        let mut expanded = Vec::with_capacity(width * 4);
        for pixel in source_row.chunks_exact(4) {
            for _ in 0..scale {
                expanded.extend_from_slice(pixel);
            }
        }
        for _ in 0..scale {
            rows.extend_from_slice(&expanded);
        }
    }
    encode_png(&rows, width as u32, width as u32, filter)
}

async fn raw_skin(state: &State, name: &str) -> Result<Vec<u8>> {
    let path = state.cache_path("raw", &format!("{name}.png"));
    if path.exists() {
        state.counters.raw_hit.fetch_add(1, Ordering::Relaxed);
        return Ok(async_fs::read(path).await?);
    }
    if let Some(pool) = &state.pool {
        match query(pool, name).await {
            Ok(meta) => match fetch_raw(state, &meta.textures.skin.url, &path).await {
                Ok(bytes) => return Ok(bytes),
                Err(error) => log::warn!("SkinSystem resolver for {name} failed: {error:#}"),
            },
            Err(error) => log::warn!("SkinSystem lookup for {name} failed: {error:#}"),
        }
    }
    if let Some(base) = &state.ely_by {
        let url = format!("{}/skins/{name}.png", base.trim_end_matches('/'));
        match fetch_raw(state, &url, &path).await {
            Ok(bytes) => return Ok(bytes),
            Err(error) => log::debug!("Ely.by resolver for {name} failed: {error:#}"),
        }
    }
    let url = resolve_mojang(state, name).await?;
    fetch_raw(state, &url, &path).await
}

struct RenderedFace {
    png: Vec<u8>,
    pixels: Option<[u8; FACE_BYTES]>,
}

fn decode_rendered_face(png_bytes: &[u8]) -> Result<[u8; FACE_BYTES]> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    if info.width != 8 || info.height != 8 || info.interlaced {
        bail!("render cache is not a non-interlaced 8x8 RGBA PNG");
    }
    let mut pixels = [0_u8; FACE_BYTES];
    reader.next_frame(&mut pixels)?;
    Ok(pixels)
}

async fn rendered_face(state: &State, name: &str) -> Result<RenderedFace> {
    let path = state.cache_path("render", &format!("{name}.png"));
    if path.exists() {
        state.counters.render_hit.fetch_add(1, Ordering::Relaxed);
        return Ok(RenderedFace {
            png: async_fs::read(path).await?,
            pixels: None,
        });
    }
    let face = decode_face_rows(&raw_skin(state, name).await?)?;
    let rendered = encode_png(&face, 8, 8, png::Filter::Sub)?;
    async_fs::write(path, &rendered).await?;
    Ok(RenderedFace {
        png: rendered,
        pixels: Some(face),
    })
}

fn response(conn: Conn, bytes: Vec<u8>, state: &'static str) -> Conn {
    conn.with_status(200)
        .with_response_header("content-type", "image/png")
        .with_response_header("cache-control", "public")
        .with_response_header("x-powered-by", "ThiccMC/renskin")
        .with_response_header("x-state", state)
        .with_body(bytes)
        .halt()
}
fn placeholder(conn: Conn, state: &State) -> Conn {
    state.counters.failed.fetch_add(1, Ordering::Relaxed);
    conn.with_status(404)
        .with_response_header("content-type", "image/png")
        .with_response_header("cache-control", "no-cache")
        .with_body(PLACEHOLDER)
        .halt()
}

#[derive(Deserialize)]
struct FaceQuery {
    username: String,
    scale: Option<u32>,
}
async fn handle_face(conn: Conn, state: Arc<State>) -> Conn {
    let query: FaceQuery = match serde_urlencoded::from_str(conn.querystring()) {
        Ok(query) => query,
        Err(_) => return conn.with_status(400).with_body("invalid query").halt(),
    };
    let name = query.username.to_lowercase();
    if !state.username_regex.is_match(&name) {
        return conn
            .with_status(400)
            .with_body("very illegal name indeed. unfortunatelly your mom...\n")
            .halt();
    }
    let scale = query
        .scale
        .filter(|scale| ALLOWED_SCALES.contains(scale))
        .unwrap_or(1);
    if scale == 1 {
        return match rendered_face(&state, &name).await {
            Ok(rendered) => response(conn, rendered.png, "rendered"),
            Err(error) => {
                log::warn!("render {name} failed: {error:#}");
                placeholder(conn, &state)
            }
        };
    }
    let path = state.cache_path("scale", &format!("{name}.{scale}.png"));
    if path.exists() {
        state.counters.scale_hit.fetch_add(1, Ordering::Relaxed);
        return match async_fs::read(path).await {
            Ok(bytes) => response(conn, bytes, "upscaled"),
            Err(error) => {
                log::warn!("scale cache {name} failed: {error:#}");
                placeholder(conn, &state)
            }
        };
    }
    match async {
        let rendered = rendered_face(&state, &name).await?;
        let face = match rendered.pixels {
            Some(face) => face,
            None => decode_rendered_face(&rendered.png)?,
        };
        let scaled = scale_and_encode(&face, scale, png::Filter::Sub)?;
        async_fs::write(&path, &scaled).await?;
        Ok::<_, anyhow::Error>(scaled)
    }
    .await
    {
        Ok(bytes) => response(conn, bytes, "upscaled"),
        Err(error) => {
            log::warn!("scale {name} failed: {error:#}");
            placeholder(conn, &state)
        }
    }
}

async fn cache_scheduler(cache_root: PathBuf, keep: Duration) {
    loop {
        trillium_smol::async_io::Timer::after(Duration::from_secs(3600)).await;
        let cutoff = SystemTime::now()
            .checked_sub(keep)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        for kind in ["raw", "render", "scale"] {
            let Ok(mut entries) = async_fs::read_dir(cache_root.join(kind)).await else {
                continue;
            };
            while let Some(Ok(entry)) = entries.next().await {
                if entry
                    .metadata()
                    .await
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .is_some_and(|m| m < cutoff)
                {
                    let _ = async_fs::remove_file(entry.path()).await;
                }
            }
        }
    }
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    femme::start();
    let cli = Cli::parse();
    let (host, port) = cli
        .bind
        .rsplit_once(':')
        .context("RENSKIN_BIND must be host:port")?;
    let port: u16 = port.parse().context("RENSKIN_BIND port must be numeric")?;
    trillium_smol::async_io::block_on(async move {
        for kind in ["raw", "render", "scale"] {
            async_fs::create_dir_all(cli.cache_root.join(kind)).await?;
        }
        let pool = match cli.database_url {
            Some(url) => match Pool::connect(&url).await {
                Ok(pool) => Some(pool),
                Err(error) => {
                    log::warn!("SkinSystem database unavailable; resolver disabled: {error:#}");
                    None
                }
            },
            None => {
                log::info!("SkinSystem database resolver disabled (DATABASE_URL is unset)");
                None
            }
        };
        let user_agent: &'static str = Box::leak(
            format!(
                "ThiccMC/renskin@{} (trillium-client)",
                env!("CARGO_PKG_VERSION")
            )
            .into_boxed_str(),
        );
        let state = Arc::new(State {
            username_regex: Regex::new(r"^[a-zA-Z0-9_]{3,16}$")?,
            pool,
            http: trillium_client::Client::new(trillium_rustls::RustlsConfig::<
                trillium_smol::ClientConfig,
            >::default()),
            user_agent,
            cache_root: cli.cache_root,
            ely_by: cli.ely_by.then_some(cli.ely_by_base),
            counters: CacheCounters::default(),
        });
        trillium_smol::async_global_executor::spawn(cache_scheduler(
            state.cache_root.clone(),
            Duration::from_secs(cli.cache_keep_hours.saturating_mul(3_600)),
        ))
        .detach();
        let handler_state = Arc::clone(&state);
        trillium_smol::config()
            .with_host(host)
            .with_port(port)
            .with_nodelay()
            .run_async(move |conn: Conn| {
                let state = Arc::clone(&handler_state);
                async move {
                    if conn.path() == "/face" {
                        handle_face(conn, state).await
                    } else {
                        conn.with_status(404).with_body("not found").halt()
                    }
                }
            })
            .await;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn straight_alpha_source_over_is_correct() {
        let mut dst = [0, 0, 255, 255];
        composite_pixel(&mut dst, &[255, 0, 0, 128]);
        assert_eq!(dst, [128, 0, 127, 255]);
    }
    #[test]
    fn fixed_scale_replicates_each_pixel() {
        let mut face = [0_u8; FACE_BYTES];
        face[..4].copy_from_slice(&[1, 2, 3, 4]);
        let mut reader = png::Decoder::new(Cursor::new(
            scale_and_encode(&face, 2, png::Filter::NoFilter).unwrap(),
        ))
        .read_info()
        .unwrap();
        let mut out = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut out).unwrap();
        assert_eq!(&out[..8], &[1, 2, 3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn stream_decoder_reads_the_face_and_hat_coordinates() {
        let mut skin = vec![0_u8; 64 * 16 * 4];
        let face = (8 * 64 + 8) * 4;
        let hat = (8 * 64 + 40) * 4;
        skin[face..face + 4].copy_from_slice(&[10, 20, 30, 255]);
        skin[hat..hat + 4].copy_from_slice(&[250, 0, 0, 128]);
        let skin_png = encode_png(&skin, 64, 16, png::Filter::NoFilter).unwrap();
        let result = decode_face_rows(&skin_png).unwrap();
        assert_eq!(&result[..4], &[130, 10, 15, 255]);
    }

    #[cfg(all(feature = "avx2", target_arch = "x86_64"))]
    #[test]
    fn avx2_dispatch_is_bit_exact_with_the_scalar_reference() {
        if std::is_x86_feature_detected!("avx2") {
            let base = [17_u8; FACE_BYTES];
            let hat = [99_u8; FACE_BYTES];
            assert_eq!(
                unsafe { composite_face_avx2(base, hat) },
                composite_face_scalar(base, hat)
            );
        }
    }
}
