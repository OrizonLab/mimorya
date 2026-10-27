//! REST APIs a company describes itself (ADR-112): the declaration of their
//! operations and what it may hold, the request a call makes of an input,
//! the answer as the model reads it, the tool each operation becomes and the
//! connector type; and the operations an OpenAPI description offers
//! (ADR-114). They are reached as every company connector is (`outbound`).

mod checks;
mod connector;
mod declaration;
mod openapi;
mod request;
mod response;
mod tool;

pub use checks::{
    MAX_CONFIG_BYTES, MAX_DESCRIPTION_CHARS, MAX_OPERATIONS, MAX_PARAMETER_DESCRIPTION_CHARS,
    MAX_PARAMETERS, MAX_PATH_CHARS, MAX_TITLE_CHARS, config_problem, header_problem,
    path_placeholders,
};
pub use connector::RestConnector;
pub use declaration::{
    HttpMethod, ParameterLocation, ParameterType, RestConfig, RestOperation, RestParameter,
    rest_tool_description, rest_tool_name, tool_namespace,
};
pub use openapi::{
    Imported, MAX_DOCUMENT_BYTES, MAX_IMPORTED_OPERATIONS, MAX_NOTES, MAX_OFFERED_BYTES,
    import_openapi,
};
pub use request::{Built, MAX_TEXT_CHARS, build, input_schema, parse_input};
pub use response::MAX_RESPONSE_BYTES;
