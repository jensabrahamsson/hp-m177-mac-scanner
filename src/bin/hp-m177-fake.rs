use clap::Parser;

#[derive(Parser)]
#[command(name = "hp-m177-fake", about = "Protocol-accurate fake M177fw (SOAP + WSD /scanner + eSCL caps)")]
struct Args {
    /// Simulate an empty ADF (default is loaded, matching a ready MFP).
    #[arg(long)]
    adf_empty: bool,
}

fn main() {
    let args = Args::parse();
    let fake = hp_m177::fake::FakeDevice::start_with(hp_m177::fake::FakeOptions {
        paper_in_adf: !args.adf_empty,
        ..hp_m177::fake::FakeOptions::default()
    })
    .expect("start fake device");
    println!("fake SOAP+eSCL on http://{}", fake.addr);
    println!("GetScannerElements POST http://{}/", fake.addr);
    println!("eSCL caps GET http://{}/eSCL/ScannerCapabilities", fake.addr);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
