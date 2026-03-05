use crate::types::RegistryAuth;
use oci_client::secrets::RegistryAuth as OciAuth;

/// Convert our auth type to oci-client auth.
pub fn to_oci_auth(auth: &RegistryAuth) -> OciAuth {
    auth.to_oci_auth()
}

/// Create a Reference from endpoint + repository + optional tag/digest.
pub fn parse_reference(
    endpoint: &str,
    repository: &str,
    reference: Option<&str>,
) -> crate::error::Result<oci_client::Reference> {
    let host = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');

    let full = match reference {
        Some(r) if r.starts_with("sha256:") => format!("{}/{}@{}", host, repository, r),
        Some(r) => format!("{}/{}:{}", host, repository, r),
        None => format!("{}/{}", host, repository),
    };

    full.parse::<oci_client::Reference>().map_err(|e| {
        crate::error::RegistryError::Oci(format!("Invalid reference '{}': {}", full, e))
    })
}
