use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::Mutex;

static CLIENT: Mutex<Option<DiscordIpcClient>> = Mutex::new(None);

const CLIENT_ID: &str = "1500619451622625371";

pub fn init() {
    println!("[Discord] Inicializando RPC...");

    let mut client = match DiscordIpcClient::new(CLIENT_ID) {
        Ok(c) => c,
        Err(e) => {
            println!("[Discord] Error creando cliente: {:?}", e);
            return;
        }
    };

    match client.connect() {
        Ok(_) => {
            println!("[Discord] Conectado correctamente");
        }
        Err(e) => {
            println!("[Discord] No se pudo conectar: {:?}", e);
            return;
        }
    }

    let payload = activity::Activity::new()
        .details("Idling...");
    match client.set_activity(payload) {
        Ok(_) => println!("[Discord] Actividad seteada"),
        Err(e) => println!("[Discord] Error seteando actividad: {:?}", e),
    }

    let mut lock = CLIENT.lock().unwrap();
    *lock = Some(client);
}

pub fn set_idle() {
    let mut lock = CLIENT.lock().unwrap();
    if let Some(client) = lock.as_mut() {
        let payload = activity::Activity::new()
            .details("Idling...");

        match client.set_activity(payload) {
            Ok(_) => println!("[Discord] Actividad: Idle"),
            Err(e) => println!("[Discord] Error: {:?}", e),
        }
    } else {
        println!("[Discord] Cliente no inicializado");
    }
}

pub fn set_playing(instance_name: &str) {
    let mut lock = CLIENT.lock().unwrap();
    if let Some(client) = lock.as_mut() {
        let details = format!("Playing {}", instance_name);
        let payload = activity::Activity::new()
            .details(&details);

        match client.set_activity(payload) {
            Ok(_) => println!("[Discord] Actividad: jugando {}", instance_name),
            Err(e) => println!("[Discord] Error: {:?}", e),
        }
    }
}