//! Drive shipped `browse_lan` against a real Bonjour advertisement.
//! A browse that returns after the first empty 250 ms poll would miss these.

use hp_m177::advertise::Advertisement;
use hp_m177::discovery::browse_lan;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn unique_instance(tag: &str) -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("hp-m177{tag}{}", n % 1_000_000_000)
}

fn saw(found: &[hp_m177::model::DiscoveredDevice], instance: &str) -> bool {
    found.iter().any(|d| {
        d.name.contains(instance) || d.name.replace('\\', "").contains(instance)
    })
}

#[test]
fn browse_lan_finds_advertised_uscan() {
    let instance = unique_instance("now");
    let _adv = Advertisement::start(free_port(), &instance).expect("advertise");
    thread::sleep(Duration::from_millis(800));
    let found = browse_lan(Duration::from_secs(3)).expect("browse_lan");
    assert!(
        saw(&found, &instance),
        "browse_lan missed {instance}. found={found:?}"
    );
}

#[test]
fn browse_lan_still_listens_after_first_empty_poll() {
    let instance = unique_instance("late");
    let port = free_port();
    let held: Arc<Mutex<Option<Advertisement>>> = Arc::new(Mutex::new(None));
    let held2 = held.clone();
    let name = instance.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(400));
        let adv = Advertisement::start(port, &name).expect("late advertise");
        *held2.lock().unwrap() = Some(adv);
    });
    let start = Instant::now();
    let found = browse_lan(Duration::from_secs(3)).expect("browse_lan");
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(350),
        "browse_lan returned in {elapsed:?}, would miss a 400ms mDNS reply"
    );
    assert!(
        saw(&found, &instance),
        "late advertisement {instance} not seen (first-timeout bug). found={found:?}"
    );
}
