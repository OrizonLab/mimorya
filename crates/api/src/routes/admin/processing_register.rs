//! The company's record of processing activities (`compliance:read`,
//! company-wide, ADR-217): drawn from its settings at each request.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use mimorya_core::processing_register::{Activity, ChosenBy, Place, Recipient};

use crate::app::AppState;
use crate::contract::admin::{
    ProcessingRegisterView, RecipientChoiceView, RecipientPlaceView, RegisterActivity,
    RegisterRecipient,
};
use crate::http::{ApiResult, iso};
use crate::plugins::Auth;
use crate::services::admin::{RegisterSources, processing_register};

pub async fn show(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<ProcessingRegisterView>> {
    let access = state.access.context(identity).await?;
    let sources = RegisterSources {
        db: &state.db,
        llm: &state.llm,
        embeddings: &state.embeddings,
        mails: state.mailer.is_some(),
    };
    let register = processing_register(sources, &access).await?;
    Ok(Json(ProcessingRegisterView {
        controller: register.controller,
        drawn_at: iso(&Utc::now()),
        activities: register.activities.into_iter().map(activity).collect(),
        measures: register.measures,
        to_complete: register.to_complete.into_iter().map(String::from).collect(),
    }))
}

fn activity(activity: Activity) -> RegisterActivity {
    RegisterActivity {
        key: activity.kind.key().to_string(),
        name: activity.name.to_string(),
        purpose: activity.purpose.to_string(),
        subjects: activity.subjects.into_iter().map(String::from).collect(),
        data: activity.data.into_iter().map(String::from).collect(),
        sensitive: activity.sensitive.map(String::from),
        recipients: activity.recipients.into_iter().map(recipient).collect(),
        retention: activity.retention,
        notes: activity.notes,
    }
}

fn recipient(recipient: Recipient) -> RegisterRecipient {
    RegisterRecipient {
        place: match recipient.place {
            Place::EuropeanUnion => RecipientPlaceView::EuropeanUnion,
            Place::OutsideEuropeanUnion => RecipientPlaceView::OutsideEuropeanUnion,
            Place::ListedByOperator => RecipientPlaceView::ListedByOperator,
            Place::DeclaredByCompany => RecipientPlaceView::DeclaredByCompany,
            Place::Operator => RecipientPlaceView::Operator,
            Place::Unknown => RecipientPlaceView::Unknown,
        },
        place_label: recipient.place.label().to_string(),
        chosen_by: match recipient.chosen_by {
            ChosenBy::Company => RecipientChoiceView::Company,
            ChosenBy::Operator => RecipientChoiceView::Operator,
        },
        chosen_by_label: recipient.chosen_by.label().to_string(),
        name: recipient.name,
        role: recipient.role,
    }
}
