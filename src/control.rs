use crate::{Message, storage::DynStorage, auth::DynAuth};
use pgp::composed::{SignedPublicKey, StandaloneSignature, Deserializable};
use std::error::Error;

#[derive(Debug, PartialEq, Eq)]
pub enum ControlCommand {
    Cancel(String),
    NewGroup { group: String },
    RmGroup(String),
}

fn parse_command(val: &str) -> Option<ControlCommand> {
    let mut parts = val.split_whitespace();
    match parts.next()?.to_ascii_lowercase().as_str() {
        "cancel" => parts.next().map(|id| ControlCommand::Cancel(id.to_string())),
        "newgroup" => parts.next().map(|g| ControlCommand::NewGroup { group: g.to_string() }),
        "rmgroup" => parts.next().map(|g| ControlCommand::RmGroup(g.to_string())),
        _ => None,
    }
}

/// Build the canonical text that was signed according to the pgpcontrol format.
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
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
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

async fn verify_pgp(
    msg: &Message,
    auth: &DynAuth,
    from: &str,
    version: &str,
    signed_headers: &str,
    sig_data: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if !auth.is_admin(from).await? {
        return Err("not admin".into());
    }
    let key_text = auth
        .get_pgp_key(from)
        .await?
        .ok_or("no key")?;
    let (key, _) = SignedPublicKey::from_string(&key_text)?;
    let armor = format!(
        "-----BEGIN PGP SIGNATURE-----\nVersion: {}\n\n{}\n-----END PGP SIGNATURE-----\n",
        version, sig_data
    );
    let (sig, _) = StandaloneSignature::from_armor_single(armor.as_bytes())?;
    let data = canonical_text(msg, signed_headers);
    sig.verify(&key, data.as_bytes())?;
    Ok(())
}

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
    let from = msg
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("From"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    let sig_header = msg
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("X-PGP-Sig"))
        .map(|(_, v)| v.clone())
        .ok_or("missing signature")?;
    let mut words = sig_header.split_whitespace();
    let version = words.next().ok_or("bad signature")?;
    let signed = words.next().ok_or("bad signature")?;
    let sig_rest = words.collect::<Vec<_>>().join("\n");
    verify_pgp(msg, auth, from, version, signed, &sig_rest).await?;
    let cmd = parse_command(&control_val).ok_or("unknown control")?;
    match cmd {
        ControlCommand::Cancel(id) => {
            storage.delete_article_by_id(&id).await?;
        }
        ControlCommand::NewGroup { group } => {
            storage.add_group(&group).await?;
        }
        ControlCommand::RmGroup(group) => {
            storage.remove_group(&group).await?;
        }
    }
    Ok(true)
}
