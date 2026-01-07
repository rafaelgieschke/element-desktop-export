use std::collections::HashMap;
use std::{env, str};

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockDecryptMut, KeyIvInit};
use base64::prelude::*;
use clap::Parser;

fn find_element_dir() -> Option<std::path::PathBuf> {
    [
        env::home_dir()?.join("snap/element-desktop/current/.config/Element/"),
        env::home_dir()?.join(".var/app/im.riot.Riot/config/Element/"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

#[derive(Parser)]
struct Cli {
    #[arg(default_value = find_element_dir().unwrap_or("".into()).into_os_string())]
    element_dir: std::path::PathBuf,
    #[arg(short, long)]
    keyring: Option<std::path::PathBuf>,
    #[arg(short, long)]
    events_db: Option<std::path::PathBuf>,
}

async fn get_secret_text(
    config: serde_json::Value,
    encrypted: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut secret_str: &str = "";
    for (key, val) in config["safeStorage"].as_object().ok_or("")? {
        if key.starts_with("seshat|")
            && let Some(val2) = val.as_str()
        {
            secret_str = val2;
        }
    }
    if !encrypted {
        return Ok(secret_str.to_owned());
    }

    let secret = BASE64_STANDARD.decode(secret_str)?;
    // https://github.com/chromium/chromium/blob/main/components/os_crypt/sync/os_crypt_posix.cc
    Ok(String::from_utf8(
        cbc::Decryptor::<aes::Aes128>::new(
            b"\xfd\x62\x1f\xe5\xa2\xb4\x02\x53\x9d\xfa\x14\x7c\xa9\x27\x27\x78".into(),
            &[b' '; 16].into(),
        )
        .decrypt_padded_vec_mut::<Pkcs7>(&secret[3..])?,
    )?)
}

async fn get_secret_gnome_libsecret(
    keyring2_path: std::path::PathBuf,
) -> Result<String, Box<dyn std::error::Error>> {
    // "GnomeKeyring\n\r\x00\n\x00\x00": https://gitlab.gnome.org/GNOME/gnome-keyring/-/blob/main/docs/file-format.txt
    // "GnomeKeyring\n\r\x00\n\x01\x00": https://gitlab.gnome.org/GNOME/libsecret/-/blob/main/libsecret/secret-file-collection.c

    let keyring = oo7::Keyring::new().await?;
    let mut secret = oo7::Secret::Text("".into());
    let attributes = HashMap::from([("app_id", "snap.element-desktop")]);
    for item in keyring.search_items(&attributes).await? {
        secret = item.secret().await?;
    }

    let keyring2 = oo7::file::Keyring::load(keyring2_path, secret).await?;
    let mut secret2 = "";
    let attributes = HashMap::from([("service", "element.io")]);
    for item in keyring2.search_items(&attributes).await? {
        if item.label().starts_with("element.io/seshat|") {
            secret = item.secret();
            secret2 = str::from_utf8(secret.as_bytes())?;
        }
    }
    Ok(secret2.to_owned())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    let config = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(
        args.element_dir.join("electron-config.json"),
    )?)?;
    // https://www.electronjs.org/docs/latest/api/safe-storage
    let secret2 = match config["safeStorageBackend"].as_str().ok_or("")? {
        "plaintext" => get_secret_text(config, false).await,
        "basic_text" => get_secret_text(config, true).await,
        "gnome_libsecret" => {
            get_secret_gnome_libsecret(
                args.keyring.unwrap_or(
                    args.element_dir
                        .join("../../.local/share/keyrings/default.keyring"),
                ),
            )
            .await
        }
        other_backend => panic!("safeStorageBackend {other_backend:?} not supported"),
    }?;

    let conn = rusqlite::Connection::open_with_flags(
        args.events_db
            .unwrap_or(args.element_dir.join("EventStore/events.db")),
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
