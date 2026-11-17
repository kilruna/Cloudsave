use std::{error::Error, fmt, io, time::Duration};

use crate::TransferControl;

pub(crate) const ATTEMPTS: u8 = 3;

#[derive(Debug)]
struct Temporary(Box<dyn Error + Send + Sync>);

impl fmt::Display for Temporary {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(output)
    }
}

impl Error for Temporary {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

pub(crate) fn temporary(error: impl Into<Box<dyn Error + Send + Sync>>) -> io::Error {
    io::Error::other(Temporary(error.into()))
}

pub(crate) fn is_temporary(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|cause| cause.is::<Temporary>())
}

pub(crate) fn run<T>(
    control: &TransferControl,
    mut operation: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    for (next, delay) in [(2, 2), (3, 5)] {
        control.checkpoint()?;
        match operation() {
            Err(error) if is_temporary(&error) => {
                control.wait_to_retry(next, Duration::from_secs(delay))?;
            }
            result => return result,
        }
    }
    control.checkpoint()?;
    match operation() {
        Err(error) if is_temporary(&error) => Err(temporary(format!(
            "WebDAV nach {ATTEMPTS} Versuchen vorübergehend nicht verfügbar: {error}"
        ))),
        result => result,
    }
}
