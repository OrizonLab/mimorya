//! The delays a tool call is given fit one another (ADR-205): a
//! connector's own call delay, then what leaving its mail server may take
//! once the answer is known (`mail_send`), stay within the delay after
//! which a run's executor stops the call and says the action may have
//! happened. Were they not to, a sent e-mail would be told perhaps sent.

use mimorya_agent::runtime::DEFAULT_TOOL_TIMEOUT;
use mimorya_connectors::DEFAULT_CALL_TIMEOUT;
use mimorya_connectors::standard_mail::LEAVE_TIMEOUT;

#[test]
fn a_call_and_its_leaving_fit_the_executor_s_delay() {
    assert!(
        DEFAULT_CALL_TIMEOUT + LEAVE_TIMEOUT < DEFAULT_TOOL_TIMEOUT,
        "{DEFAULT_CALL_TIMEOUT:?} + {LEAVE_TIMEOUT:?} must stay within {DEFAULT_TOOL_TIMEOUT:?}"
    );
}
