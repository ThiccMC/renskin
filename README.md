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
- [x] SIMD
  > Holy f, do not use it in prod! It might
  > - `STATUS_HEAP_CORRUPTION`
  >
  > please build with `cargo build --release --no-default-features`
- [ ] Support premium skin
  > [!NOTE]
  > Not yet, might need thirdparty :sob:

## Requirements

- `SkinRestorer` dataset

## How to use

> Compile it yourself

- Clone
- Config with .env
- `cargo install --path .`

## Best practice

- crontab `*/3 * * * * rm .cache/moj/*`
