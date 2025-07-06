use crate::{Message, auth::DynAuth, storage::DynStorage};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use pgp::composed::{Deserializable, SignedPublicKey, StandaloneSignature};
use sha2::{Digest, Sha256, Sha512};
use std::error::Error;

#[derive(Debug, PartialEq, Eq)]
pub enum ControlCommand {
    Cancel(String),
    NewGroup { group: String, moderated: bool },
    RmGroup(String),
}

fn parse_command(val: &str) -> Option<ControlCommand> {
    let mut parts = val.split_whitespace();
    match parts.next()?.to_ascii_lowercase().as_str() {
        "cancel" => parts
            .next()
            .map(|id| ControlCommand::Cancel(id.to_string())),
        "newgroup" => {
            let group = parts.next()?;
            let moderated = parts
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("moderated"));
            Some(ControlCommand::NewGroup {
                group: group.to_string(),
                moderated,
            })
        }
        "rmgroup" => parts.next().map(|g| ControlCommand::RmGroup(g.to_string())),
        _ => None,
    }
}

fn parse_elements(val: &str) -> Vec<(String, String)> {
    val.split_whitespace()
        .filter_map(|p| {
            let p = p.trim_matches(',');
            p.split_once(':')
                .map(|(s, v)| (s.to_ascii_lowercase(), v.to_string()))
        })
        .collect()
}

fn hash_key(scheme: &str, key: &str) -> Option<String> {
    let bytes = key.as_bytes();
    let digest = match scheme {
        "sha256" => Sha256::digest(bytes).to_vec(),
        "sha512" => Sha512::digest(bytes).to_vec(),
        "sha1" => sha1::Sha1::digest(bytes).to_vec(),
        _ => return None,
    };
    Some(STANDARD.encode(digest))
}

fn verify_cancel(keys: &[(String, String)], locks: &[(String, String)]) -> bool {
    for (scheme, key) in keys {
        if let Some(hash) = hash_key(scheme, key) {
            for (ls, lv) in locks {
                if scheme.eq_ignore_ascii_case(ls) && *lv == hash {
                    return true;
                }
            }
        }
    }
    false
}

/// Build the canonical text that was signed according to the pgpcontrol format.
#[must_use]
pub fn canonical_text(msg: &Message, signed_headers: &str) -> String {
    let mut out = String::new();
    out.push_str("X-Signed-Headers: ");
    out.push_str(signed_headers);
    out.push('\n');
    for name in signed_headers.split(',') {
        out.push_str(name);
        out.push_str(": ");
        let val = msg
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map_or("", |(_, v)| v.as_str());
        out.push_str(val);
        out.push('\n');
    }
    out.push('\n');
    for line in msg.body.replace("\r\n", "\n").split_inclusive('\n') {
        if line.starts_with('-') {
            out.push_str("- ");
        }
        out.push_str(line);
    }
    if !msg.body.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Verify a PGP signature on a message.
///
/// # Errors
///
/// Returns an error if the signature verification fails or if there are issues
/// with key retrieval or parsing.
pub async fn verify_pgp(
    msg: &Message,
    auth: &DynAuth,
    user: &str,
    version: &str,
    signed_headers: &str,
    sig_data: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let key_text = auth.get_pgp_key(user).await?.ok_or("no key")?;
    let (key, _) = SignedPublicKey::from_string(&key_text)?;
    let armor = format!(
        "-----BEGIN PGP SIGNATURE-----\nVersion: {version}\n\n{sig_data}\n-----END PGP SIGNATURE-----\n"
    );
    let (sig, _) = StandaloneSignature::from_armor_single(armor.as_bytes())?;
    let data = canonical_text(msg, signed_headers);
    sig.verify(&key, data.as_bytes())?;
    Ok(())
}

/// Handle control messages for newsgroup management.
///
/// # Errors
///
/// Returns an error if there's a problem processing the control message,
/// such as database errors or authentication failures.
pub async fn handle_control(
    msg: &Message,
    storage: &DynStorage,
    auth: &DynAuth,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let control_val = match msg
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Control"))
    {
        Some((_, v)) => v.clone(),
        None => return Ok(false),
    };
    let cmd = parse_command(&control_val).ok_or("unknown control")?;

    if let ControlCommand::Cancel(ref id) = cmd {
        // try Cancel-Key authentication first
        if let Some((_, key_val)) = msg
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Cancel-Key"))
        {
            if let Some(orig) = storage.get_article_by_id(id).await? {
                if let Some((_, lock_val)) = orig
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("Cancel-Lock"))
                {
                    let keys = parse_elements(key_val);
                    let locks = parse_elements(lock_val);
                    if verify_cancel(&keys, &locks) {
                        storage.delete_article_by_id(id).await?;
                    }
                    return Ok(true);
                }
            }
            return Ok(true);
        }
    }

    // fall back to admin-signed control message
    let from = msg
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("From"))
        .map_or("", |(_, v)| v.as_str());
    let sig_header = msg
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("X-PGP-Sig"))
        .map(|(_, v)| v.clone())
        .ok_or("missing signature")?;
    if !auth.is_admin(from).await? {
        return Err("not admin".into());
    }
    let mut words = sig_header.split_whitespace();
    let version = words.next().ok_or("bad signature")?;
    let signed = words.next().ok_or("bad signature")?;
    let sig_rest = words.collect::<Vec<_>>().join("\n");
    verify_pgp(msg, auth, from, version, signed, &sig_rest).await?;
    match cmd {
        ControlCommand::Cancel(id) => {
            storage.delete_article_by_id(&id).await?;
        }
        ControlCommand::NewGroup { group, moderated } => {
            storage.add_group(&group, moderated).await?;
        }
        ControlCommand::RmGroup(group) => {
            storage.remove_group(&group).await?;
        }
    }
    Ok(true)
}
