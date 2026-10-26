//! Inbound webhooks a person sets for their agent (ADR-109): their own
//! hooks (`hooks`), and what another system posts to one (`delivery`).

mod delivery;
mod hooks;

pub use delivery::{MAX_BODY_BYTES, MAX_PER_HOUR, Refusal, deliver, turn_text, unknown};
pub use hooks::{
    HookChanges, HookInput, MAX_HOOKS, TOKEN_PREFIX, change_hook, create_hook, delete_hook,
    inbound_hooks, replace_token,
};
