fn main() {
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-env-changed=WIFI_SSID");
    println!("cargo:rerun-if-env-changed=WIFI_PASS");
    println!("cargo:rerun-if-env-changed=COAP_PORT");

    // Prefer shell env; fall back to crate-root `.env` when present.
    let _ = dotenvy::dotenv();

    let ssid = std::env::var("WIFI_SSID").unwrap_or_else(|_| {
        panic!(
            "WIFI_SSID must be set via `.env` or the environment.\n\
             Copy `.env.example` to `.env` and fill in your network credentials."
        )
    });
    let pass = std::env::var("WIFI_PASS").unwrap_or_else(|_| {
        panic!(
            "WIFI_PASS must be set via `.env` or the environment \
             (use an empty value for open networks)."
        )
    });
    let port = std::env::var("COAP_PORT").unwrap_or_else(|_| "5683".into());
    port.parse::<u16>().unwrap_or_else(|_| {
        panic!("COAP_PORT must be a valid u16 (got {port:?})")
    });

    // Escape is not applied; keep credentials free of newlines.
    if ssid.contains('\n') || ssid.contains('\0') || pass.contains('\n') || pass.contains('\0') {
        panic!("WIFI_SSID / WIFI_PASS must not contain newline or NUL");
    }

    println!("cargo:rustc-env=WIFI_SSID={ssid}");
    println!("cargo:rustc-env=WIFI_PASS={pass}");
    println!("cargo:rustc-env=COAP_PORT={port}");
}
