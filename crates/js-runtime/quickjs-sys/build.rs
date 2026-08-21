use std::path::PathBuf;

fn main() {
    let vendor = PathBuf::from("vendor/quickjs-ng");

    let sources = ["quickjs.c", "dtoa.c", "libregexp.c", "libunicode.c"];
    for src in &sources {
        println!("cargo:rerun-if-changed={}", vendor.join(src).display());
    }
    println!("cargo:rerun-if-changed={}", vendor.join("quickjs.h").display());

    let mut build = cc::Build::new();
    build.include(&vendor).define("_GNU_SOURCE", None);

    if cfg!(windows) {
        build
            .define("WIN32_LEAN_AND_MEAN", None)
            .define("_WIN32_WINNT", "0x0601")
            // quickjs.c uses C11 atomics; MSVC needs an explicit C11 standard
            // plus the experimental atomics switch to accept them.
            .flag_if_supported("/std:c11")
            .flag_if_supported("/experimental:c11atomics");
    }

    for src in &sources {
        build.file(vendor.join(src));
    }

    build.compile("quickjs");
}
