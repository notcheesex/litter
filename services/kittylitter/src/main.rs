fn main() -> anyhow::Result<()> {
    alleycat::App {
        binary_name: "kittylitter",
        qualifier: "com",
        organization: "sigkitten",
        application: "kittylitter",
        label: "com.sigkitten.kittylitter",
        version: option_env!("KITTYLITTER_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
    }
    .run()
}
