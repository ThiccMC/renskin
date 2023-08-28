# `renskin`

An alternate to `SkinSystem` for `SkinRestorer`. By default, it have less overhead task since `SkinSystem` render the whole texture on a real 3D enviroment, and this thing only render the face and hat on a 2D canvas.

## Roadmap

- [x] It works!
- [x] TODO: Implement a proper SQL foolproof
  > With proper rustegexp!
- [x] It function as a HTTP server
- [-] It have proper caching
  > TODO: It know when to rebake new image but
  >
  > - It must know when to flush the images
  > - It must let the proxy know when to cache (Edge)
- [ ] Support premium skin
  > [!NOTE]
  > Not yet, might need thirdparty :sob:

## Requirements

- `SkinRestorer` dataset

## How to use

> Compile it yourself (it require database connection upon compile time, for stupid typecheck whatever thanks)

- Clone
- Config with .env
- `cargo install --path .`

## Best practice

- crontab `*/3 * * * * rm .cache/moj/*`
