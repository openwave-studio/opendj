---
title: Build from source
---

You need a recent stable Rust, CMake and a C++ compiler. On Linux you also need:

```sh
sudo apt install cmake libfontconfig-dev libxkbcommon-dev libasound2-dev libudev-dev libclang-dev
```

Then, from a clone of the
[repository](https://github.com/openwave-studio/opendj):

```sh
cargo run --release
```

## Building these docs

The docs are a [Starlight](https://starlight.astro.build) site in
`website/docs`, built with Node 22 or newer. To preview them as you edit:

```sh
cd website/docs
npm install
npm run dev
```
