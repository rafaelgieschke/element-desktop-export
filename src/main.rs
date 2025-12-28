use std::collections::HashMap;
use std::{env, str};

use clap::Parser;

#[derive(Parser)]
struct Cli {
    #[arg(default_value = env::home_dir().unwrap().join("snap/element-desktop/current/.local/share/keyrings/default.keyring").into_os_string())]
    keyring: std::path::PathBuf,
    #[arg(default_value = env::home_dir().unwrap().join("snap/element-desktop/current/.config/Element/EventStore/events.db").into_os_string())]
    events_db: std::path::PathBuf,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    // "GnomeKeyring\n\r\x00\n\x00\x00": https://gitlab.gnome.org/GNOME/gnome-keyring/-/blob/main/docs/file-format.txt
    // "GnomeKeyring\n\r\x00\n\x01\x00": https://gitlab.gnome.org/GNOME/libsecret/-/blob/main/libsecret/secret-file-collection.c

    let keyring = oo7::Keyring::new().await?;
    let mut secret = oo7::Secret::Text("".into());
    let attributes = HashMap::from([("app_id", "snap.element-desktop")]);
    for item in keyring.search_items(&attributes).await? {
        secret = item.secret().await?;
    }

    let keyring2 = oo7::file::Keyring::load(args.keyring, secret).await?;
    let mut secret2 = "";
    let attributes = HashMap::from([("service", "element.io")]);
    for item in keyring2.search_items(&attributes).await? {
        if item.label().starts_with("element.io/seshat|") {
            secret = item.secret();
            secret2 = str::from_utf8(secret.as_bytes())?;
        }
    }

    let conn = rusqlite::Connection::open_with_flags(
        args.events_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.pragma_update(None, "key", secret2)?;

    // See `create_tables` in https://github.com/matrix-org/seshat/blob/main/src/database/static_methods.rs
    for row in conn
        .prepare("SELECT source FROM events")?
        .query_map([], |row| Ok(row.get::<_, String>(0)?))?
    {
        println!("{}", row?);
    }

    Ok(())
}
