use clap::Parser;
use hp_m177::model::DEFAULT_BRIDGE_PORT;

#[derive(Parser)]
#[command(name = "hp-m177-bridge", about = "Local eSCL / AirScan facade for the M177fw")]
struct Args {
    #[arg(long, default_value_t = DEFAULT_BRIDGE_PORT)]
    port: u16,
    #[arg(long)]
    bind: Option<String>,
    #[arg(long)]
    no_advertise: bool,
}

fn main() {
    let args = Args::parse();
    let bind = args.bind.unwrap_or_else(|| format!("0.0.0.0:{}", args.port));
    let store = match hp_m177::Store::from_env_or_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("hp-m177-bridge: {e}");
            std::process::exit(1);
        }
    };
    let device = store.default_device().ok();
    let facade = match hp_m177::facade::EsclFacade::bind(&bind, device) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("hp-m177-bridge: {e}");
            std::process::exit(1);
        }
    };
    facade.refresh_adf_from_device();
    println!(
        "eSCL listening on {}/eSCL/ScannerCapabilities",
        facade.url()
    );
    if !args.no_advertise {
        match hp_m177::advertise::Advertisement::start(
            facade.addr.port(),
            hp_m177::APP_DISPLAY_NAME,
        ) {
            Ok(_a) => {
                println!("Advertised _uscan._tcp rs=eSCL is=platen,adf cs=color,grayscale");
                // Keep advertisement alive for the process lifetime.
                std::mem::forget(_a);
            }
            Err(e) => eprintln!("Bonjour advertise failed: {e}"),
        }
    }
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
