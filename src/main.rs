#![feature(portable_simd)]

use image::{codecs::png::PngEncoder, ImageBuffer, Rgb};
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

mod ihacks;
use ihacks::comp;

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
    raw_path: &Path,
    met: &AvatarMeta,
) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>, tide::Error> {
    let buff = if raw_path.exists() {
        ImageReader::open(raw_path)?
            .with_guessed_format()?
            .decode()?
    } else {
        let b = fetch(&met, raw_path).await?;
        let buf = Cursor::new(b);
        ImageReader::new(buf).with_guessed_format()?.decode()?
    }
    .as_rgba8()
    .unwrap()
    .to_owned();

    let mut canvas = RgbImage::new(8, 8);

    comp(8, 8, 8, 8, &mut canvas, &buff);
    comp(40, 8, 8, 8, &mut canvas, &buff);

    Ok(canvas)
}

#[derive(Deserialize)]
struct PlayerQuery {
    username: String,
}

fn face_err(not_ok_str: String) -> tide::Result {
    Ok(Response::builder(404)
        .header(
            "X-Not-Ok",
            match not_ok_str.as_str() {
                "no rows returned by a query that expected to return at least one row" => {
                    "no entry"
                }
                _ => {
                    println!("{not_ok_str}");
                    ""
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

    if !res.state().username_regex.is_match(&name) {
        return Ok(Response::builder(400)
            .body("very illegal name indeed. unfortunatelly your mom...\n")
            .build());
    }

    let _fpth = format!("./.cache/ren/{}.png", name);
    let face_path = Path::new(&_fpth);

    if !face_path.exists() {
        let url = env::var("DATABASE_URL")?;
        let pool = MySqlPool::connect(&url).await?;
        let query = query(&pool, &name).await;
        if let Ok(meta) = query {
            let _pth = format!("./.cache/moj/{}.png", &meta.profile_id);

            let raw_path = Path::new(&_pth);
            if !raw_path.exists() || !face_path.exists() {
                let f = File::create(face_path)?;
                let enc = PngEncoder::new(f);
                let _r = draw_face(raw_path, &meta).await?;
                _r.write_with_encoder(enc)?;
            };
        } else {
            return face_err(query.err().map(|e| format!("{e}")).unwrap_or_default());
        }
    }
    return Ok(Response::builder(200)
        .header("X-Powered-By", "ThiccMC/renskin")
        .header("Cache-Control", "public")
        .header("Content-Type", "image/png")
        .body(Body::from_file(face_path).await?)
        .build());
}

#[derive(Clone)]
struct State {
    username_regex: Regex,
}
// #[tokio::main]
#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv()?;

    fs::create_dir_all("./.cache/moj")?;
    fs::create_dir_all("./.cache/ren")?;

    let mut app = tide::with_state(State {
        // a case where there is absolutely no uppercase
        username_regex: Regex::new(r"^[a-zA-Z0-9_]{3,16}$")?,
    });

    let bind = env::var("RENSKIN_BIND").unwrap_or("127.0.0.1:3727".to_string());

    println!("bind ur server at {}, modify with RENSKIN_BIND. gl", bind);

    app.at("/face").get(face);
    app.listen(bind).await?;

    Ok(())
}
