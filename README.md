# cargo-cocoapods - Build Rust code for Xcode integration

## Installing

```
cargo install cargo-cocoapods
```

You'll also need to install all the toolchains you intend to use. Simplest way is with the following:

```
rustup target add \
    x86_64-apple-darwin \
    aarch64-apple-darwin \
    x86_64-apple-ios \
    aarch64-apple-ios \
    aarch64-apple-ios-sim
```

Modify as necessary for your use case.

## Usage

Type `cargo pod --help` for information.

### Supported hosts

- macOS (x86_64 and arm64)

## Similar projects

* [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) - for building Android libraries
* [cargo-lipo](https://github.com/TimNN/cargo-lipo) - for building iOS universal Rust libraries

## License

This project is licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

---

[Uyghurs are under attack in Xinjiang.](https://foreignpolicy.com/2019/12/30/xinjiang-crackdown-uighur-2019-what-happened/) The Chinese government is placing millions of people into indoctrination camps and engaging in forced labour.
