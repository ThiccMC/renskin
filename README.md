# `renskin`

An alternate to `SkinSystem` for `SkinRestorer`. By default, it have less overhead task since `SkinSystem` render the whole texture on a real 3D enviroment, and this thing only render the face and hat on a 2D canvas.

## Roadmap

- [x] It works!
- [x] TODO: Implement a proper SQL foolproof
  > With proper rustegexp!
- [x] It function as a HTTP server
- [x] It have proper caching
  > TODO: It know when to rebake new image but
  >
  > - It must know when to flush the images (bash-scripted)
  > - It must let the proxy know when to cache (Edge) (50%)
- [x] Fixed sqlx macro shills (hack)
- [x] Streaming PNG face compositor
  > Decode rows 8..15 only, compose RGBA8 with a portable scalar reference.
  > `--features avx2` enables runtime-dispatched row loads on capable x86_64.
- [ ] Support premium skin
  > [!NOTE]
  > Not yet, might need thirdparty :sob:

> Because the rendering stays at 300ms (composition only, with tested conditions)
> and the upscaling stays at 40-70ms, it is hard to implement more stuff

## Requirements

- `SkinRestorer` dataset

## How to use

> Compile it yourself

- Clone
- Config with .env
- `cargo build --release`

The service uses the Mojang profile/session APIs by default. An optional
SkinSystem MySQL resolver is enabled with `DATABASE_URL`; a failed connection
only emits a warning and falls back. Add `--ely-by` to try
`http://skinsystem.ely.by/skins/{username}.png` before Mojang. Run
`rskd --help` for all Clap options.

## Best practice

- Cache eviction runs inside the Smol executor once per hour. Set
  `RENSKIN_CACHE_DIR` for an explicit writable cache location (the container
  defaults to `/tmp/renskin-cache`).
