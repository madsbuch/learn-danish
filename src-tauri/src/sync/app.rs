//! What makes this app's sync its own: the names and ports that keep it from
//! ever meeting another app built on the same code (Polski) on the same
//! network. Porting sync to another app changes this file and nothing else.

/// The app's name, as messages to the learner say it.
pub const APP_NAME: &str = "Dansk";

/// Discovery messages start with this (`DANSK/1 Q …`, `DANSK/1 R …`).
pub const DISCOVERY_PROTOCOL: &str = "DANSK/1";

/// What a vault's id is hashed with to make its discovery tag: two apps
/// with the same vault id (they never have one) would still not answer
/// each other.
pub const VAULT_TAG_PREFIX: &str = "dansk-vault:";

/// The UDP port every device answers discovery on.
pub const DISCOVERY_PORT: u16 = 47831;

/// The TCP port a device listens for its vault on, if it is free (any free
/// one otherwise: discovery says which).
pub const SYNC_PORT: u16 = 47832;

/// What a pairing code (the QR) starts with: a code from another app, or a
/// later version of this one, is refused by name.
pub const PAIRING_SCHEME: &str = "dansk-pair:1:";

/// Prepended to every Noise handshake as its prologue, so a handshake with
/// a device of another app fails even if everything else lines up.
pub const NOISE_PROLOGUE: &[u8] = b"dansk-sync/1";
