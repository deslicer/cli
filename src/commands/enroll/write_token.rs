use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::errors::CliError;

pub fn require_write_file_if_not_tty(
    is_tty: bool,
    write_file: Option<&Path>,
) -> Result<(), CliError> {
    if is_tty || write_file.is_some() {
        return Ok(());
    }
    Err(CliError::Other(
        "stdout is not a terminal; pass --write-file PATH to store the one-time token (0600, must not exist)".into(),
    ))
}

pub fn reject_unsafe_write_path(path: &Path) -> Result<(), CliError> {
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(CliError::Other(
            "--write-file path must not contain '..'".into(),
        ));
    }
    Ok(())
}

pub fn write_token_file(path: &Path, token: &str) -> Result<(), CliError> {
    reject_unsafe_write_path(path)?;
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            CliError::Other(format!(
                "--write-file {} already exists (refusing to overwrite)",
                path.display()
            ))
        } else {
            CliError::Other(format!("write token file: {err}"))
        }
    })?;
    file.write_all(token.as_bytes())
        .map_err(|err| CliError::Other(format!("write token file: {err}")))?;
    file.write_all(b"\n")
        .map_err(|err| CliError::Other(format!("write token file: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_tty_requires_write_file() {
        let err = require_write_file_if_not_tty(false, None).expect_err("require");
        assert!(err.to_string().contains("--write-file"));
        require_write_file_if_not_tty(true, None).expect("tty");
        require_write_file_if_not_tty(false, Some(Path::new("/tmp/token"))).expect("path");
    }

    #[test]
    fn write_file_is_exclusive_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("enroll.token");
        write_token_file(&path, "dsle_enroll_test-only-not-a-credential").expect("write");
        let again = write_token_file(&path, "dsle_enroll_test-only-not-a-credential");
        assert!(again
            .expect_err("exists")
            .to_string()
            .contains("already exists"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let parent_escape = dir.path().join("..").join("escaped.token");
        assert!(reject_unsafe_write_path(&parent_escape).is_err());
    }
}
