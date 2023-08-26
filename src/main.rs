use image::{io::Reader as ImageReader, RgbImage};
use serde::Deserialize;
use sqlx::{MySql, MySqlPool, Pool};
use std::{env, error::Error, fs, io::Cursor, path::Path};

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

async fn fetcheur(pool: &Pool<MySql>, nick: &String) -> Result<AvatarMeta, Box<dyn Error>> {
    let oq = sqlx::query!(
        "
SELECT Skin AS skin
FROM Players
WHERE Nick = ?
LIMIT 1
",
        nick
    )
    .fetch_all(pool)
    .await?;

    let skn = &oq[0].skin;
    let sq = sqlx::query!(
        "
SELECT FROM_BASE64(`Value`) as data
FROM Skins
WHERE Nick = ?
ORDER BY timestamp DESC
LIMIT 1
        ",
        skn
    )
    .fetch_all(pool)
    .await?;
    let sqd = sq[0].data.as_ref().unwrap().to_owned(); //stolen
    let sqr = String::from_utf8(sqd)?;
    let met: AvatarMeta = serde_json::from_str(&sqr)?;
    Ok(met)
}

async fn fetch(met: &AvatarMeta, path: &Path) -> Result<bytes::Bytes, Box<dyn Error>> {
    let url = &met.textures.skin.url;
    let res = reqwest::get(url).await?.bytes().await?;
    fs::write(path, &res)?;
    Ok(res)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv()?;

    fs::create_dir_all("./.cache/moj")?;
    fs::create_dir_all("./.cache/ren")?;

    let pool = MySqlPool::connect(&env::var("DATABASE_URL")?).await?;

    let name = "hUwUtao".to_lowercase();

    let met = fetcheur(&pool, &name).await?;
    let fmtp = format!("./.cache/moj/{}.png", &met.profile_id);

    let draw_path = Path::new(&fmtp);

    let buff = if draw_path.exists() {
        ImageReader::open(draw_path)?
            .with_guessed_format()?
            .decode()?
    } else {
        let b = fetch(&met, draw_path).await?;
        let buf = Cursor::new(b);
        ImageReader::new(buf).with_guessed_format()?.decode()?
    }
    .as_rgba8()
    .unwrap()
    .to_owned();

    let mut canvas = RgbImage::new(8, 8);

    comp(8, 8, 8, 8, &mut canvas, &buff);
    comp(40, 8, 8, 8, &mut canvas, &buff);

    canvas.save(format!("./.cache/ren/{}.png", name))?;

    Ok(())
}
