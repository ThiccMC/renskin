#![feature(portable_simd)]
#![feature(stdarch_x86_avx512)]

use image::{
    codecs::png::PngEncoder, GenericImage, GenericImageView, ImageBuffer, Pixel, Rgb, Rgba,
    RgbaImage,
};
use regex::Regex;
use tide::{Body, Middleware, Request, Response};

use image::{ImageReader, RgbImage};
use serde::Deserialize;
use sqlx::{MySql, MySqlPool, Pool};
use std::{
    env,
    error::Error,
    fmt::Debug,
    fs::{self, File},
    io::Cursor,
    path::Path,
    str::Bytes,
};
use tide_prometheus::prometheus::{
    register_counter, register_int_counter, register_int_counter_vec, Counter, IntCounterVec, Opts,
};

#[cfg(feature = "simd")]
mod ihacks;
#[cfg(feature = "simd")]
use ihacks::comp;

const RESTRICTED_SIZE: [u32; 5] = [1u32, 2u32, 4u32, 8u32, 16u32];

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
    #[serde(alias = "profileId")]
    profile_id: String,
    textures: TextureListMeta,
}

static PLACEHOLDER: &'static [u8] = include_bytes!("placeholder.png");

async fn query(pool: &Pool<MySql>, nick: &String) -> Result<AvatarMeta, tide::Error> {
    let sq: sqlx::Result<(String, i32)> = sqlx::query_as(
        // "SELECT CONVERT(FROM_BASE64(sk.Value) USING UTF8) as data, 0 as t
        //     FROM Skins as sk
        //     WHERE sk.Nick = ?
        //     LIMIT 1",
        "SELECT CONVERT(FROM_BASE64(sk.Value) USING UTF8) as data, 0 as t
        FROM Players AS pl
        INNER JOIN Skins AS sk
        ON pl.Skin = sk.Nick
        WHERE pl.Nick = ?
        LIMIT 1",
    )
    .bind(nick)
    .fetch_one(pool)
    .await;
    return if let Ok((sqd, _)) = sq {
        Ok(serde_json::from_str::<AvatarMeta>(&sqd)?)
    } else {
        if let Some(er) = sq.err() {
            Err(tide::Error::from_str(404, format!("{}", er)))
        } else {
            Err(tide::Error::from_str(404, "unk err"))
        }
    };
}

async fn fetch(met: &AvatarMeta, path: &Path) -> Result<Vec<u8>, tide::Error> {
    let url = &met.textures.skin.url;
    // let res = reqwest::get(url).await?.bytes().await?;
    let res = surf::get(url).await?.body_bytes().await?;
    fs::write(path, &res)?;
    Ok(res)
}

async fn draw_face(
    state: &State,
    raw_path: &Path,
    met: &AvatarMeta,
) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, tide::Error> {
    let mut buff = if raw_path.exists() {
        state.counter_cache.with_label_values(&["hit_raw"]);
        ImageReader::open(raw_path)?
            .with_guessed_format()?
            .decode()?
    } else {
        state.counter_cache.with_label_values(&["missed"]).inc();
        let b = fetch(&met, raw_path).await?;
        let buf = Cursor::new(b);
        ImageReader::new(buf).with_guessed_format()?.decode()?
    }
    .as_rgba8()
    .unwrap()
    .to_owned();

    let layers = vec![
        buff.sub_image(8, 8, 8, 8).to_image(),
        buff.sub_image(40, 8, 8, 8).to_image(),
    ];

    #[cfg(not(feature = "simd"))]
    {
        let mut canvas = RgbaImage::new(8, 8);
        for (x, y, p) in canvas.enumerate_pixels_mut() {
            for l in &layers {
                p.blend(l.get_pixel(x, y));
            }
        }
        Ok(canvas)
    }

    #[cfg(feature = "simd")]
    {
        let mut canvas = [0u8; 256];

        comp(8, 8, &mut canvas, &buff);
        comp(40, 8, &mut canvas, &buff);

        Ok(RgbaImage::from_raw(8, 8, canvas.to_vec()).unwrap())
    }
}

#[derive(Deserialize)]
struct PlayerQuery {
    username: String,
    scale: Option<u32>,
}

fn face_err(state: &State, not_ok_str: String) -> tide::Result {
    state.counter_cache.with_label_values(&["failed"]);
    Ok(Response::builder(404)
        .header(
            "X-Not-Ok",
            match not_ok_str.as_str() {
                "no rows returned by a query that expected to return at least one row" => {
                    "no entry"
                }
                _ => {
                    println!("{not_ok_str}");
                    "yeah not ok"
                }
            },
        )
        .header("Cache-Control", "no-cache")
        .header("Content-Type", "image/png")
        .body(PLACEHOLDER)
        .build())
}

async fn face(res: Request<State>) -> tide::Result {
    let rq: PlayerQuery = res.query()?;

    let name = rq.username.to_lowercase();
    let scale = rq
        .scale
        .filter(|f| (1..128).contains(f) && RESTRICTED_SIZE.contains(f))
        .unwrap_or(1u32);

    if !res.state().username_regex.is_match(&name) {
        return Ok(Response::builder(400)
            .body("very illegal name indeed. unfortunatelly your mom...\n")
            .build());
    }

    let _fpth = format!("./.cache/ren/{}.png", name);
    let face_path = Path::new(&_fpth);

    let _spth = format!("./.cache/scl/{name}.{scale}.png");
    let scale_path = Path::new(&_spth);
    let should_upscale = scale > 1 && !scale_path.exists();
    let mut cached_1x_buffer: Option<RgbaImage> = None;
    let mut cache_hit = false;

    if !face_path.exists() {
        let url = env::var("DATABASE_URL")?;
        let pool = MySqlPool::connect(&url).await?;
        let query = query(&pool, &name).await;
        if let Ok(meta) = query {
            let _pth = format!("./.cache/moj/{}.png", &meta.profile_id);

            let raw_path = Path::new(&_pth);

            let f = File::create(face_path)?;
            let enc = PngEncoder::new(f);
            cached_1x_buffer = Some(draw_face(res.state(), raw_path, &meta).await?);
            cached_1x_buffer.clone().unwrap().write_with_encoder(enc)?;
        } else {
            return face_err(
                res.state(),
                query
                    .err()
                    .map(|e: surf::Error| format!("{e}"))
                    .unwrap_or_default(),
            );
        }
    } else {
        cache_hit = true;
        if should_upscale {
            cached_1x_buffer = Some(
                ImageReader::open(face_path)?
                    .with_guessed_format()?
                    .decode()?
                    .into(),
            )
        }
    }

    if scale != 1 {
        if should_upscale {
            let f = File::create(scale_path)?;
            let enc = PngEncoder::new_with_quality(
                f,
                image::codecs::png::CompressionType::Best,
                image::codecs::png::FilterType::Paeth,
            );
            let mut canvas = RgbaImage::new(8u32 * scale, 8u32 * scale);
            let cached = cached_1x_buffer.as_ref().unwrap();
            for (x, y, p) in canvas.enumerate_pixels_mut() {
                *p = *cached.get_pixel(x / scale, y / scale);
            }
            canvas.write_with_encoder(enc)?;
        }
        if cache_hit {
            res.state().counter_cache.with_label_values(&["hit_scl"]);
        }
        return Ok(Response::builder(200)
            .header("X-Powered-By", "ThiccMC/renskin")
            .header("X-State", "upscaled")
            .header("Cache-Control", "public")
            .header("Content-Type", "image/png")
            .body(Body::from_file(scale_path).await?)
            .build());
    } else {
        if cache_hit {
            res.state().counter_cache.with_label_values(&["hit_rend"]);
        }
        return Ok(Response::builder(200)
            .header("X-Powered-By", "ThiccMC/renskin")
            .header("X-State", "rendered")
            .header("Cache-Control", "public")
            .header("Content-Type", "image/png")
            .body(Body::from_file(face_path).await?)
            .build());
    }
}

#[derive(Clone)]
struct State {
    username_regex: Regex,
    counter_cache: IntCounterVec,
}
// #[tokio::main]
#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv()?;

    fs::create_dir_all("./.cache/moj")?;
    fs::create_dir_all("./.cache/ren")?;
    fs::create_dir_all("./.cache/scl")?;

    let mut app = tide::with_state(State {
        // a case where there is absolutely no uppercase
        username_regex: Regex::new(r"^[a-zA-Z0-9_]{3,16}$")?,
        counter_cache: register_int_counter_vec!(
            Opts::new("rsk_cache", "Cacheness of request"),
            &["status"]
        )?,
    });

    let bind = env::var("RENSKIN_BIND").unwrap_or("127.0.0.1:3727".to_string());

    println!("bind ur server at {}, modify with RENSKIN_BIND. gl", bind);

    app.with(tide_prometheus::Prometheus::new("rsk"));
    app.at("/metrics").get(tide_prometheus::metrics_endpoint);
    app.at("/face").get(face);
    app.listen(bind).await?;

    Ok(())
}
