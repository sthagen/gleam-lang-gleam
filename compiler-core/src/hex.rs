use std::path::PathBuf;

use crate::{
    io::{FileSystemIO, HttpClient},
    Error,
};

// TODO: abstract away the IO portion of this module so that a HTTP client or
// Hex client is injected in, or something like that...
// The current design of the hex client is... not amazing. It tries to be clever
// but you can't build a trait object with it, making it rather awkward.

pub struct Client {
    fs: Box<dyn FileSystemIO>,
    http: Box<dyn HttpClient>,
    config: hexpm::Config,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("fs", &"<FileSystemWriter>")
            .finish()
    }
}

impl Client {
    pub fn new(fs: Box<dyn FileSystemIO>, http: Box<dyn HttpClient>) -> Self {
        Self {
            fs,
            http,
            config: hexpm::Config::new(),
        }
    }

    pub async fn ensure_package_downloaded(
        &self,
        package_name: &str,
        version: &str,
        checksum: &[u8],
    ) -> Result<(), Error> {
        let tarball_path = package_cache_tarball_path(package_name, version);
        if self.fs.is_file(&tarball_path) {
            tracing::info!(
                package = package_name,
                version = version,
                "Package already downloaded"
            );
            return Ok(());
        }
        tracing::info!(
            package = package_name,
            version = version,
            "Downloading package"
        );

        let request = hexpm::get_package_tarball_request(package_name, version, None, &self.config);
        let response = self.http.send(request.map(String::into_bytes)).await?;

        let tarball = hexpm::get_package_tarball_response(response, checksum).map_err(|error| {
            Error::DownloadPackageError {
                package_name: package_name.to_string(),
                package_version: version.to_string(),
                error: error.to_string(),
            }
        })?;
        let mut file = self.fs.open(&tarball_path)?;
        file.write(&tarball)?;
        Ok(())
    }
}

fn package_cache_tarball_path(package_name: &str, version: &str) -> PathBuf {
    packages_cache_dir().join(format!("{}-{}.tar", package_name, version))
}

fn packages_cache_dir() -> PathBuf {
    gleam_cache_dir().join("hex").join("hexpm").join("packages")
}

fn gleam_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .expect("Failed to determine user cache directory")
        .join("gleam")
}

#[test]
fn paths() {
    assert!(gleam_cache_dir().ends_with("gleam"));

    assert!(packages_cache_dir().ends_with("gleam/hex/hexpm/packages"));

    assert!(package_cache_tarball_path("gleam_stdlib", "0.17.1")
        .ends_with("gleam/hex/hexpm/packages/gleam_stdlib-0.17.1.tar"));

    assert!(package_cache_tarball_path("elli", "1.0.0")
        .ends_with("gleam/hex/hexpm/packages/elli-1.0.0.tar"));
}
