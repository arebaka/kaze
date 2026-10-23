fn main() {
    println!("cargo:rerun-if-changed=../../ui/app.slint");
    let mut config = slint_build::CompilerConfiguration::new();
    if std::env::var("PROFILE").as_deref() == Ok("debug") {
        config = config.with_debug_info(true);
    }
    slint_build::compile_with_config("../../ui/app.slint", config).expect("slint compile failed");
}
