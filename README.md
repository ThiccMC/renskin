# `renskin`

An alternate to `SkinSystem` for `SkinRestorer`. By default, it have less overhead task since `SkinSystem` render the whole texture on a real 3D enviroment, and this thing only render the face and hat on a 2D canvas.

## Roadmap

- [x] It works!
- [ ] TODO: Implement a proper SQL foolproof
- [ ] It function as a HTTP server
- [ ] It have proper caching

## Requirements

- `SkinRestorer` dataset

## How to use

> Compile it yourself (it require database connection upon compile time, for stupid typecheck whatever thanks)

- Clone
- Config with .env
- `cargo install --path .`