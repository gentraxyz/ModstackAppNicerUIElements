use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static CLIENT: Mutex<Option<DiscordIpcClient>> = Mutex::new(None);
static LAST_ACTIVITY: Mutex<u64> = Mutex::new(0);
static IS_PLAYING: Mutex<bool> = Mutex::new(false);

const CLIENT_ID: &str = "1500619451622625371";
const AFK_TIMEOUT_SECS: u64 = 15 * 60; // 15 minutes

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn update_last_activity() {
    let mut last = LAST_ACTIVITY.lock().unwrap();
    *last = now_secs();
}

pub fn init() {
    println!("[Discord] Initializing RPC...");

    let mut client = match DiscordIpcClient::new(CLIENT_ID) {
        Ok(c) => c,
        Err(e) => {
            println!("[Discord] Error creating client: {:?}", e);
            return;
        }
    };

    match client.connect() {
        Ok(_) => println!("[Discord] Connected successfully"),
        Err(e) => {
            println!("[Discord] Could not connect: {:?}", e);
            return;
        }
    }

    let payload = activity::Activity::new().details("Browsing...");
    match client.set_activity(payload) {
        Ok(_) => println!("[Discord] Activity set"),
        Err(e) => println!("[Discord] Error setting activity: {:?}", e),
    }

    let mut lock = CLIENT.lock().unwrap();
    *lock = Some(client);
    drop(lock);

    update_last_activity();

    std::thread::spawn(|| {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));

            let is_playing = *IS_PLAYING.lock().unwrap();
            if is_playing {
                continue;
            }

            let last = *LAST_ACTIVITY.lock().unwrap();
            let elapsed = now_secs().saturating_sub(last);

            if elapsed >= AFK_TIMEOUT_SECS {
                let mut lock = CLIENT.lock().unwrap();
                if let Some(client) = lock.as_mut() {
                    let payload = activity::Activity::new().details("AFK...");
                    match client.set_activity(payload) {
                        Ok(_) => println!("[Discord] Activity: AFK"),
                        Err(e) => println!("[Discord] Error setting AFK: {:?}", e),
                    }
                }
            }
        }
    });
}

pub fn set_idle() {
    update_last_activity();

    let mut playing = IS_PLAYING.lock().unwrap();
    *playing = false;
    drop(playing);

    let mut lock = CLIENT.lock().unwrap();
    if let Some(client) = lock.as_mut() {
        let payload = activity::Activity::new().details("Idling...");
        match client.set_activity(payload) {
            Ok(_) => println!("[Discord] Activity: Idle"),
            Err(e) => println!("[Discord] Error: {:?}", e),
        }
    } else {
        println!("[Discord] Client not initialized");
    }
}

pub fn set_playing(instance_name: &str) {
    update_last_activity();

    let mut playing = IS_PLAYING.lock().unwrap();
    *playing = true;
    drop(playing);

    let mut lock = CLIENT.lock().unwrap();
    if let Some(client) = lock.as_mut() {
        let details = format!("Playing {}", instance_name);
        let payload = activity::Activity::new().details(&details);
        match client.set_activity(payload) {
            Ok(_) => println!("[Discord] Activity: playing {}", instance_name),
            Err(e) => println!("[Discord] Error: {:?}", e),
        }
    }
}