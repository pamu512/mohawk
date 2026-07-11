use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use thiserror::Error;

/// Application-wide error type serialized to the frontend over Tauri IPC.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("io error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("{0}")]
    InternalError(String),
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Self::InternalError(err.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let (kind, message) = match self {
            AppError::DatabaseError(e) => ("DatabaseError", e.to_string()),
            AppError::SerializationError(e) => ("SerializationError", e.to_string()),
            AppError::IOError(e) => ("IOError", e.to_string()),
            AppError::InternalError(msg) => ("InternalError", msg.clone()),
        };

        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", kind)?;
        state.serialize_field("message", &message)?;
        state.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_structured_ipc_payload() {
        let err = AppError::InternalError("invalid FSRS state".into());
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(
            value,
            json!({
                "kind": "InternalError",
                "message": "invalid FSRS state",
            })
        );
    }

    #[test]
    fn maps_io_error_variant() {
        let err = AppError::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "mohawk.db",
        ));
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["kind"], "IOError");
        assert!(value["message"].as_str().unwrap().contains("mohawk.db"));
    }
}
