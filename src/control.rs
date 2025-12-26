use crate::{
    Message,
    auth::{
        DynAuth,
        pgp_discovery::{DefaultPgpKeyDiscovery, PgpKeyDiscovery},
    },
    storage::DynStorage,
};
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use pgp::native::{Deserializable, SignedPublicKey, StandaloneSignature};
use sha2::{Digest, Sha256, Sha512};
use std::io::Cursor;

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

/// Check if a message is a control message.
pub fn is_control_message(msg: &Message) -> bool {
    msg.headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("Control") && !v.trim().is_empty())
}

/// Verify a PGP signature on a message.
///
/// This function attempts to verify a PGP signature using a stored key.
/// If no key is stored or verification fails, it will attempt to discover
/// the key from key servers and update the stored key if discovery succeeds
/// and verification with the new key is successful.
///
/// # Errors
///
/// Returns an error if the signature verification fails even after attempting
/// key discovery, or if there are issues with key retrieval or parsing.
pub async fn verify_pgp(
    msg: &Message,
    auth: &DynAuth,
    user: &str,
    version: &str,
    signed_headers: &str,
    sig_data: &str,
    key_servers: &[String],
) -> Result<()> {
    // Create PGP key discovery instance with configured servers
    let discovery = DefaultPgpKeyDiscovery::with_key_servers(key_servers.to_vec());

    // First try with existing stored key
    let stored_key = auth.get_pgp_key(user).await?;

    if let Some(key_text) = &stored_key
        && let Ok(verification_result) =
            try_verify_with_key(msg, key_text, version, signed_headers, sig_data).await
        && verification_result.is_ok()
    {
        return Ok(());
    }
    // If verification failed with stored key, try discovery

    // Attempt key discovery if no key exists or verification failed
    match discovery.discover_key(user).await? {
        Some(discovered_key) => {
            // Try verification with discovered key
            match try_verify_with_key(msg, &discovered_key, version, signed_headers, sig_data)
                .await?
            {
                Ok(()) => {
                    // Verification succeeded, update stored key
                    if let Err(e) = auth.update_pgp_key(user, &discovered_key).await {
                        // Log at info without username for GDPR, debug with username
                        tracing::info!(error = %e, "Failed to update PGP key");
                        tracing::debug!(user = user, error = %e, "Failed to update PGP key details");
                        // Continue anyway since verification succeeded
                    }
                    Ok(())
                }
                Err(e) => {
                    // Discovery found a key but verification still failed
                    // Keep original key if it existed
                    Err(anyhow::anyhow!(
                        "Signature verification failed even with discovered key: {e}"
                    ))
                }
            }
        }
        None => {
            // No key could be discovered
            if stored_key.is_some() {
                Err(anyhow::anyhow!(
                    "Signature verification failed with stored key and no alternative key could be discovered"
                ))
            } else {
                Err(anyhow::anyhow!(
                    "No PGP key found for user and no key could be discovered"
                ))
            }
        }
    }
}

/// Try to verify a signature with a specific key.
async fn try_verify_with_key(
    msg: &Message,
    key_text: &str,
    version: &str,
    signed_headers: &str,
    sig_data: &str,
) -> Result<Result<()>> {
    let (key, _) = SignedPublicKey::from_string(key_text)?;
    let armor = format!(
        "-----BEGIN PGP SIGNATURE-----\nVersion: {version}\n\n{sig_data}\n-----END PGP SIGNATURE-----\n"
    );
    let (sig, _) = StandaloneSignature::from_armor_single(Cursor::new(armor.as_bytes()))?;
    let data = canonical_text(msg, signed_headers);

    match sig.verify(&key, data.as_bytes()) {
        Ok(()) => Ok(Ok(())),
        Err(e) => Ok(Err(e.into())),
    }
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
    config: &crate::config::Config,
) -> Result<bool> {
    let control_val = match msg
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Control"))
    {
        Some((_, v)) => v.clone(),
        None => return Ok(false),
    };
    let cmd = parse_command(&control_val).ok_or_else(|| anyhow::anyhow!("unknown control"))?;

    if let ControlCommand::Cancel(ref id) = cmd {
        // try Cancel-Key authentication first
        if let Some((_, key_val)) = msg
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Cancel-Key"))
        {
            if let Some(orig) = storage.get_article_by_id(id).await?
                && let Some((_, lock_val)) = orig
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
        .ok_or_else(|| anyhow::anyhow!("missing signature"))?;
    if !auth.is_admin(from).await? {
        return Err(anyhow::anyhow!("not admin"));
    }
    let mut words = sig_header.split_whitespace();
    let version = words
        .next()
        .ok_or_else(|| anyhow::anyhow!("bad signature"))?;
    let signed = words
        .next()
        .ok_or_else(|| anyhow::anyhow!("bad signature"))?;
    let sig_rest = words.collect::<Vec<_>>().join("\n");
    verify_pgp(
        msg,
        auth,
        from,
        version,
        signed,
        &sig_rest,
        &config.pgp_key_servers,
    )
    .await?;
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
