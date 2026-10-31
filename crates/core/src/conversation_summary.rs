//! When a conversation's older messages are folded into its summary
//! (ADR-145): the history replayed to the model is the summary, then the
//! latest messages it does not cover yet, as many as the history window
//! holds. Once those grow past a count or a size, the oldest are folded in;
//! the latest exchanges stay word for word, and the newest message is never
//! folded, nor half of an exchange when a whole one can be kept. A summary
//! that lags behind (more than the window holds) is caught up by passes
//! chained at a run's end, a few at most (ADR-149); until then, the
//! messages between it and the window are neither summarised nor reread.

/// Messages replayed word for word at most after the summary: the history
/// window.
pub const REPLAYED_MESSAGES: usize = 20;
/// Messages not covered yet past which the oldest are folded: below the
/// history window, so that a summary keeps it from cutting anything.
pub const FOLD_AFTER_MESSAGES: usize = 16;
/// Their characters past which the oldest are folded, whatever their count.
pub const FOLD_AFTER_CHARS: usize = 60_000;
/// The latest messages kept word for word, at most…
pub const KEPT_MESSAGES: usize = 6;
/// …and within these characters, but never fewer than `MIN_KEPT`.
pub const KEPT_CHARS: usize = 20_000;
/// The last exchange, kept whatever its size.
pub const MIN_KEPT: usize = 2;
/// Messages folded in one pass at most: the rest waits for the next.
pub const MAX_FOLDED: usize = 24;
/// Passes chained at a run's end at most, while its conversation's summary
/// lags (ADR-149): about as many messages as a conversation shows; the rest
/// waits for the next run's end.
pub const MAX_PASSES: usize = 8;

/// Of the `after_summary` messages a summary does not cover yet, those the
/// history leaves out: neither summarised nor replayed, until passes catch
/// up.
pub fn not_replayed(after_summary: usize) -> usize {
    after_summary.saturating_sub(REPLAYED_MESSAGES)
}

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Person,
    Assistant,
}

/// A message the summary does not cover yet, as the rule weighs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unfolded {
    pub speaker: Speaker,
    pub chars: usize,
}

/// How many of `messages` (those not covered yet, oldest first) to fold
/// into the summary now: none until they grow past `FOLD_AFTER_MESSAGES`
/// or `FOLD_AFTER_CHARS`.
pub fn messages_to_fold(messages: &[Unfolded]) -> usize {
    let total: usize = messages.iter().map(|m| m.chars).sum();
    if messages.len() <= FOLD_AFTER_MESSAGES && total <= FOLD_AFTER_CHARS {
        return 0;
    }
    // The latest messages, from the newest, while they fit.
    let mut kept = 0;
    let mut kept_chars = 0;
    for message in messages.iter().rev() {
        let fits = kept < KEPT_MESSAGES && kept_chars + message.chars <= KEPT_CHARS;
        if kept >= MIN_KEPT && !fits {
            break;
        }
        kept += 1;
        kept_chars += message.chars;
    }
    let mut folded = messages.len().saturating_sub(kept);
    // An answer goes with its question: the kept part opens on a person's
    // message when one can be, else on what the rule kept.
    while folded > 0 && messages[folded].speaker == Speaker::Assistant {
        if messages.len() - (folded + 1) < MIN_KEPT {
            break;
        }
        folded += 1;
    }
    if folded <= MAX_FOLDED {
        return folded;
    }
    // Bounded, the pass stops before a person's message, so that what the
    // next pass folds opens on it too; a run of answers with no question in
    // reach is cut at the bound.
    let mut bound = MAX_FOLDED;
    while bound > 0 && messages[bound].speaker == Speaker::Assistant {
        bound -= 1;
    }
    if bound > 0 { bound } else { MAX_FOLDED }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(count: usize, chars: usize) -> Vec<Unfolded> {
        (0..count)
            .map(|i| Unfolded {
                speaker: if i % 2 == 0 {
                    Speaker::Person
                } else {
                    Speaker::Assistant
                },
                chars,
            })
            .collect()
    }

    #[test]
    fn nothing_is_folded_below_the_count_and_the_size() {
        assert_eq!(messages_to_fold(&[]), 0);
        assert_eq!(messages_to_fold(&exchange(16, 3_000)), 0);
        assert_eq!(messages_to_fold(&exchange(4, 15_000)), 0);
    }

    #[test]
    fn past_the_count_the_latest_three_exchanges_stay() {
        assert_eq!(messages_to_fold(&exchange(18, 100)), 12);
    }

    #[test]
    fn past_the_size_the_latest_messages_stay_within_theirs() {
        // 5 000 characters each: four fit in the kept 20 000.
        assert_eq!(messages_to_fold(&exchange(14, 5_000)), 10);
        // A long last exchange stays whole, whatever its size.
        let mut long = exchange(6, 100);
        long.extend(exchange(2, 40_000));
        assert_eq!(messages_to_fold(&long), 6);
    }

    #[test]
    fn the_kept_part_opens_on_a_person_s_message_when_it_can() {
        // Ending on a request: the six latest would open on an answer,
        // which goes with its folded question.
        assert_eq!(messages_to_fold(&exchange(17, 100)), 12);
        // Never below the last exchange to get there: the minimum keeps
        // an answer and the request after it.
        let mut messages = exchange(10, 100);
        messages.extend(exchange(3, 25_000));
        assert_eq!(messages_to_fold(&messages), 11);
    }

    #[test]
    fn a_bounded_pass_keeps_an_answer_with_its_question() {
        // A request left unanswered shifts the exchanges after it: at the
        // bound, an answer whose question would be folded.
        let mut messages = vec![Unfolded {
            speaker: Speaker::Person,
            chars: 100,
        }];
        messages.extend(exchange(29, 100));
        assert_eq!(messages[MAX_FOLDED].speaker, Speaker::Assistant);
        let folded = messages_to_fold(&messages);
        assert_eq!(folded, MAX_FOLDED - 1);
        assert_eq!(messages[folded].speaker, Speaker::Person);
        // Answers alone up to the bound: cut there all the same.
        let answers = vec![
            Unfolded {
                speaker: Speaker::Assistant,
                chars: 100,
            };
            40
        ];
        assert_eq!(messages_to_fold(&answers), MAX_FOLDED);
    }

    #[test]
    fn one_pass_folds_a_bounded_number_and_never_the_newest_message() {
        assert_eq!(messages_to_fold(&exchange(60, 100)), MAX_FOLDED);
        let alone = vec![Unfolded {
            speaker: Speaker::Person,
            chars: FOLD_AFTER_CHARS + 1,
        }];
        assert_eq!(messages_to_fold(&alone), 0);
    }

    #[test]
    fn what_a_lagging_summary_leaves_out_is_neither_summarised_nor_replayed() {
        assert_eq!(not_replayed(0), 0);
        assert_eq!(not_replayed(REPLAYED_MESSAGES), 0);
        assert_eq!(not_replayed(REPLAYED_MESSAGES + 1), 1);
        assert_eq!(not_replayed(202), 182);
    }
}
