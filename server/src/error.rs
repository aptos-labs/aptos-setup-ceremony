use hyper::StatusCode;


/// The point of this struct is to have an error which knows which HTTP code to return.
#[derive(Debug)]
pub struct ErrorWithCode {
    pub error: anyhow::Error,
    pub code: Option<StatusCode>,
}

impl ErrorWithCode {
    pub fn context(self, s: &str) -> ErrorWithCode {
        Self {
            error: self.error.context(String::from(s)),
            code: self.code,
        }
    }

    pub fn code(&self) -> StatusCode {
        self.code.unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl<T> From<T> for ErrorWithCode
where
    T: Into<anyhow::Error>,
{
    fn from(error: T) -> Self {
        Self {
            error: error.into(),
            code: None,
        }
    }
}

pub fn bad_request(error: anyhow::Error) -> ErrorWithCode {
    ErrorWithCode {
        error,
        code: Some(StatusCode::BAD_REQUEST),
    }
}

pub fn server_error(error: anyhow::Error) -> ErrorWithCode {
    ErrorWithCode {
        error,
        code: Some(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub fn service_unavailable(error: anyhow::Error) -> ErrorWithCode {
    ErrorWithCode {
        error,
        code: Some(StatusCode::SERVICE_UNAVAILABLE),
    }
}

/// Trait to easily convert results into results that know a code to return.
/// If the wrapped error type already knows its code, do not override.
pub trait UseCodeOnError<T> {
    fn use_code_on_error(self, code: StatusCode) -> Result<T, ErrorWithCode>;
}

impl<T> UseCodeOnError<T> for Result<T, anyhow::Error> {
    fn use_code_on_error(self, code: StatusCode) -> Result<T, ErrorWithCode> {
        self.map_err(|error| ErrorWithCode {
            error,
            code: Some(code),
        })
    }
}

// TODO: is this trait necessary?
impl<T> UseCodeOnError<T> for Result<T, ErrorWithCode> {
    fn use_code_on_error(self, _code: StatusCode) -> Result<T, ErrorWithCode> {
        self
    }
}




#[macro_export]
macro_rules! bail {
    ($msg:literal $(,)?) => {
        return anyhow::Result::Err($crate::error::ErrorWithCode{error: anyhow::anyhow!($msg), code: None})
    };
    ($err:expr $(,)?) => {
        return anyhow::Result::Err($crate::error::ErrorWithCode{anyhow::anyhow!($err), code: None})
    };
    ($fmt:expr, $($arg:tt)*) => {
        return anyhow::Result::Err($crate::error::ErrorWithCode{error: anyhow::anyhow!($fmt, $($arg)*), code: None})
    };
}
