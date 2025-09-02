use http::{HeaderValue, Method};
use openssl::base64;
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Public};
use openssl::rsa::{Padding, Rsa};
use openssl::sign::{Signer, Verifier};
use hyper::body::Bytes;
use std::collections::HashMap;
use std::fmt::Display;
use chrono::{DateTime, Utc};
use chrono_tz::Etc::GMT;
use url::Url;
use crate::activitypub::objects::actor::Actor;
use crate::local::actors::LocalActorStub;
use crate::error::{self, MainsailError};
use crate::external;
use crate::local::InternalResult;

#[derive(Debug)]
struct SignatureFields {
    key_id: Url,
    decoded_signature: Vec<u8>,
    expected_signature: String,
}

#[derive(Debug, PartialEq)]
enum DigestHashAlgorithm {
    Sha256
}

#[derive(Debug)]
struct Digest {
    algorithm: DigestHashAlgorithm,
    value: String,
}

fn fail(message: &str) -> MainsailError {
    MainsailError::Unauthorized(message.into())
}

fn now() -> DateTime<chrono_tz::Tz> {
    Utc::now().with_timezone(&GMT)
}

impl Digest {
    fn make(bytes: &[u8]) -> Self {
        let vec = openssl::sha::sha256(bytes);
        let value = base64::encode_block(&vec);
        Digest { algorithm: DigestHashAlgorithm::Sha256, value }
    }

    fn from_str(digest_str: &str) -> InternalResult<Digest> {
        let (hash_algorithm, value) = digest_str.split_once("=")
            .ok_or(error::unauthorized(format!("Malformatted digest: {digest_str}")))?;

        if hash_algorithm.to_lowercase() != "sha-256" {
            let message = format!("Unsupported hash algorithm: {hash_algorithm}");
            return Err(error::unauthorized(message));
        }

        let digest = Digest { algorithm: DigestHashAlgorithm::Sha256, value: value.to_string() };
        Ok(digest)
    }
}

impl PartialEq for Digest {
    fn eq(&self, other: &Self) -> bool {
        self.algorithm == other.algorithm && self.value == other.value
    }
}

impl Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hash = match self.algorithm {
            DigestHashAlgorithm::Sha256 => "SHA-256"
        };
        write!(f, "{}={}", hash, &self.value)
    }
}

pub fn generate_pkey() -> InternalResult<PKey<openssl::pkey::Private>> {
    let rsa = Rsa::generate(2048).map_err(MainsailError::OpenSSLError)?;
    PKey::from_rsa(rsa).map_err(MainsailError::OpenSSLError)
}

pub fn generate_pkey_string() -> InternalResult<String> {
    let pkey = generate_pkey()?;
    let bytes = pkey.private_key_to_pem_pkcs8()?;
    String::from_utf8(bytes).map_err(|_| MainsailError::InternalPkeyError)
}

pub fn sign(mut req: reqwest::Request, sender: &LocalActorStub) -> InternalResult<reqwest::Request> {
    let LocalActorStub { domain: _, id, pkey } = sender;
    let key_id = format!("{id}#main-key");

    // These are internal server errors because we're the ones who made the req
    let method = req.method();
    let host = req.url().host().ok_or(MainsailError::new_from_str("Missing host in req"))?;
    let target = req.url().path();
    let date = req.headers().get("date")
        .ok_or(MainsailError::new_from_str("Missing date in req"))?
        .to_str()
        .map_err(|_| { MainsailError::new_from_str("Invalid date in req") })?;

    let mut digest = None;
    if let Some(body) = req.body() {
        let bytes = body.as_bytes().ok_or(MainsailError::new_from_str("Invalid body provided"))?;
        digest = Some(Digest::make(bytes));
    }

    let mut unsigned_signature = format!(
        "(request-target): {} {}\nhost: {}\ndate: {}",
        method.to_string().to_lowercase(), target, host, date
    );
    if let Some(digest) = &digest {
        let digest = format!("\ndigest: {digest}");
        unsigned_signature.push_str(&digest)
    }

    let mut signer = Signer::new(MessageDigest::sha256(), pkey)?;
    signer.set_rsa_padding(Padding::PKCS1).unwrap();
    signer.update(unsigned_signature.as_bytes())?;
    let signed_signature = signer.sign_to_vec()?;

    let signed_and_encoded_signature = base64::encode_block(&signed_signature);

    let digest_header_str = if digest.is_some() { " digest" } else { "" };
    let signature_header_value = format!(
        r#"keyId="{key_id}",headers="(request-target) host date{digest_header_str}",signature="{signed_and_encoded_signature}""#,
    );

    let signature_header_value = HeaderValue::from_str(&signature_header_value)
        .map_err(|_| { MainsailError::InternalInvalidHeader })?;

    req.headers_mut().append("signature", signature_header_value);
    if let Some(digest) = digest {
        let digest_header_value = HeaderValue::from_str(&digest.to_string())
            .map_err(|_| { MainsailError::InternalInvalidHeader })?;
        req.headers_mut().append("digest", digest_header_value);
    }

    Ok(req)
}

pub async fn validate_activitypub_request(req: &hyper::Request<Bytes>, server: &LocalActorStub) -> InternalResult<Actor> {
    let fields = parse_signature_fields(req)?;

    // This relies on the whole actor being available at the public key URL
    // Which is the case for mastodon, at least, but I have no idea if that's reliable
    let actor = external::get_external_actor_no_cache(server, &fields.key_id).await.map_err(|_| {
        error::unauthorized(format!("Failed to retrieve actor at {}", &fields.key_id))
    })?;

    let key_string = &actor.public_key.public_key_pem;
    let public_key = PKey::public_key_from_pem(key_string.as_bytes()).map_err(|_| {
        error::unauthorized(format!("Failed to parse public key {} from PEM string", &fields.key_id))
    })?;

    // This will error if the signature is invalid
    // We pass the errors from this function on directly because they're specific
    validate_signature(req, fields, &public_key)?;

    Ok(actor)
}

fn parse_signature_fields(req: &hyper::Request<Bytes>) -> InternalResult<SignatureFields> {
    let request_target = format!("{} {}", req.method().as_str().to_lowercase(), req.uri());
    let signature = req.headers().get("signature")
        .ok_or(fail("Missing signature"))?
        .to_str()
        .map_err(|_| { fail("Invalid encoding for signature") })?;

    let values = signature.split(",").map(|s| {
        let (key, value) = s.split_once("=").ok_or(fail("Failed to parse signature"))?;
        // This removes the quotes
        let len = value.len();
        let value = &value[1..len - 1];
        Ok((key, value))
    }).collect::<Result<HashMap<&str, &str>, MainsailError>>()?;

    let key_id: Url = values
        .get("keyId")
        .ok_or(fail("Missing key_id value"))?
        .parse()
        .map_err(|_| { fail("Invalid key_id") })?;

    let actual_signature = values.get("signature").ok_or(fail("Parsed signature header is missing the actual signature"))?;
    let decoded_signature = base64::decode_block(actual_signature)?;
    let headers = values.get("headers").ok_or(fail("Parsed signature header is missing the headers list"))?;

    let header_pairs = headers
        .split(' ')
        .map(|header_name| {
            if header_name == "(request-target)" {
                return Ok(("(request-target)", request_target.as_str()));
            }

            let header_value = req.headers()
                .get(header_name)
                .ok_or(fail(&format!("Missing expected header: {header_name}")))?;
            let header_value = header_value.to_str().map_err(|_| { fail("Invalid header") })?;
            Ok((header_name, header_value))
        }).collect::<Result<Vec<(&str, &str)>, MainsailError>>()?;

    let expected_signature = header_pairs
        .iter()
        .map(|(name, value)| { format!("{name}: {value}") })
        .collect::<Vec<String>>()
        .join("\n");

    let signature_fields = SignatureFields { key_id, decoded_signature, expected_signature };
    Ok(signature_fields)
}

fn validate_signature(req: &hyper::Request<Bytes>, fields: SignatureFields, public_key: &PKey<Public>) -> InternalResult<()> {
    let actual_signature = fields.decoded_signature.as_slice();

    let date = req.headers().get("date")
        .ok_or(fail("Missing date from request"))?
        .to_str()
        .map_err(|_| fail("Invalid date provided in request"))?;
    let date = chrono::NaiveTime::parse_from_str(date, "%a, %d %b %Y %X %Z")
        .map_err(|_| { fail("Unable to parse date string") })?;
    let now = now().time();
    let diff = now - date;
    if diff.num_hours() > 12 {
        return Err(fail("More than 12 hours elapsed since request"));
    }

    let mut verifier = Verifier::new(MessageDigest::sha256(), public_key)?;
    verifier.set_rsa_padding(Padding::PKCS1)?;
    verifier.update(fields.expected_signature.as_bytes())?;

    let passed = verifier.verify(actual_signature).map_err(MainsailError::OpenSSLError)?;
    if !passed {
        return Err(fail("Signature failed to match"));
    }

    if req.method() != Method::GET {
        let expected_digest = Digest::make(req.body());
        let actual_digest = req.headers().get("digest").ok_or(fail("Missing digest header"))?;
        let actual_digest = actual_digest.to_str().map_err(|_| fail("Invalid digest header"))?;
        let actual_digest = Digest::from_str(actual_digest)?;
        if expected_digest != actual_digest {
            return Err(fail("Digest failed to match"));
        }
    }

    Ok(())
}

pub fn get_ap_date() -> String {
    let date = Utc::now().with_timezone(&GMT);
    date.format("%a, %d %b %Y %X %Z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::{Duration, Utc};
    use http::{Method, Request, Uri};
    use hyper::body::Bytes;
    use http::header::{ACCEPT, DATE, HeaderValue, HOST};

    use openssl::pkey::PKey;

    use crate::activitypub::signature::get_ap_date;
    use crate::local::actors::LocalActorStub;
    use crate::tests::get_private_key;

    const ACTOR_ID: &str = "https://example.com/_mainsail/actors/10";
    const EXPECTED_KEY_ID: &str = "https://example.com/_mainsail/actors/10#main-key";
    static SHORT_ACCEPT_HEADER: HeaderValue = HeaderValue::from_static("application/activity+json");

    #[test]
    fn signature_match_get() {
        let uri = Uri::from_static("https://example.com/users/happy_hippo/inbox");
        let host = "example.com";
        let sender = LocalActorStub {
            id: ACTOR_ID.to_string(),
            domain: host.to_string(),
            pkey: get_private_key(),
        };
        let date = get_ap_date();

        let client = reqwest::Client::new();
        let request_to_send = client
            .request(Method::GET, uri.to_string())
            .header(DATE, HeaderValue::from_str(&date).unwrap())
            .header(HOST, host)
            .header(ACCEPT, &SHORT_ACCEPT_HEADER)
            .build()
            .unwrap();

        let request_to_send = sign(request_to_send, &sender).unwrap();
        let signature_header = request_to_send.headers().get("signature").unwrap();

        let receive_request = Request::builder()
            .method("GET")
            .uri(uri.path())
            .header("date", date)
            .header("host", host)
            .header("signature", signature_header)
            .header(ACCEPT, &SHORT_ACCEPT_HEADER)
            .body(Bytes::new())
            .unwrap();

        let fields = parse_signature_fields(&receive_request).unwrap();

        assert_eq!(fields.key_id.as_str(), EXPECTED_KEY_ID);

        let public_key_pem = sender.pkey.public_key_to_pem().unwrap();
        let public_key = PKey::public_key_from_pem(public_key_pem.as_slice()).unwrap();
        super::validate_signature(&receive_request, fields, &public_key).unwrap();
    }

    #[test]
    fn signature_match_post() {
        let uri = Uri::from_static("https://example.com/users/happy_hippo/inbox");
        let host = "example.com";
        let sender = LocalActorStub {
            id: ACTOR_ID.to_string(),
            domain: host.to_string(),
            pkey: get_private_key(),
        };
        let date = get_ap_date();

        let body = Bytes::from_static(b"{ status: \"ok\" }");
        let digest = Digest::make(&body);

        let client = reqwest::Client::new();
        let request_to_send = client
            .request(Method::POST, uri.to_string())
            .header(DATE, HeaderValue::from_str(&date).unwrap())
            .header(HOST, host)
            .header(ACCEPT, &SHORT_ACCEPT_HEADER)
            .body(body.clone())
            .build()
            .unwrap();

        let request_to_send = sign(request_to_send, &sender).unwrap();
        let signature_header = request_to_send.headers().get("signature").unwrap();

        let receive_request = Request::builder()
            .method("POST")
            .uri(uri.path())
            .header("date", date)
            .header("host", host)
            .header("digest", digest.to_string())
            .header("signature", signature_header)
            .header(ACCEPT, &SHORT_ACCEPT_HEADER)
            .body(body)
            .unwrap();

        let fields = parse_signature_fields(&receive_request).unwrap();

        assert_eq!(fields.key_id.as_str(), EXPECTED_KEY_ID);

        let public_key_pem = sender.pkey.public_key_to_pem().unwrap();
        let public_key = PKey::public_key_from_pem(public_key_pem.as_slice()).unwrap();

        super::validate_signature(&receive_request, fields, &public_key).unwrap();
    }

    #[test]
    fn signature_fail_digest_header_does_not_match_signature() {
        let uri = Uri::from_static("https://example.com/users/happy_hippo/inbox");
        let host = "example.com";
        let sender = LocalActorStub {
            id: ACTOR_ID.to_string(),
            domain: host.to_string(),
            pkey: get_private_key(),
        };
        let date = get_ap_date().to_string();
        let body = Bytes::from_static(b"{ status: \"ok\" }");

        let client = reqwest::Client::new();
        let request_to_send = client
            .request(Method::POST, uri.to_string())
            .header(DATE, HeaderValue::from_str(&date).unwrap())
            .header(HOST, host)
            .header(ACCEPT, &SHORT_ACCEPT_HEADER)
            .body(body.clone())
            .build()
            .unwrap();

        let request_to_send = sign(request_to_send, &sender).unwrap();
        let signature_header = request_to_send.headers().get("signature").unwrap();

        let receive_request = Request::builder()
            .method("POST")
            .uri(uri.path())
            .header("date", &date)
            .header("host", host)
            .header("digest", "bad digest")
            .header("signature", signature_header)
            .header(ACCEPT, &SHORT_ACCEPT_HEADER)
            .body(body)
            .unwrap();

        let fields = parse_signature_fields(&receive_request).unwrap();

        assert_eq!(fields.key_id.as_str(), EXPECTED_KEY_ID);

        let public_key_pem = sender.pkey.public_key_to_pem().unwrap();
        let public_key = PKey::public_key_from_pem(public_key_pem.as_slice()).unwrap();

        super::validate_signature(&receive_request, fields, &public_key).unwrap_err();
    }

    #[test]
    fn signature_fail_date_too_old() {
        let uri = Uri::from_static("https://example.com/users/happy_hippo/inbox");
        let host = "example.com";
        let sender = LocalActorStub {
            id: ACTOR_ID.to_string(),
            domain: host.to_string(),
            pkey: get_private_key(),
        };
        let date = Utc::now().naive_utc() - Duration::hours(3);
        let date = date.to_string();

        let body = Bytes::from_static(b"{ status: \"ok\" }");
        let digest = Digest::make(&body);

        let client = reqwest::Client::new();
        let request_to_send = client
            .request(Method::POST, uri.to_string())
            .header(DATE, HeaderValue::from_str(&date).unwrap())
            .header(HOST, host)
            .header(ACCEPT, &SHORT_ACCEPT_HEADER)
            .body(body.clone())
            .build()
            .unwrap();

        let request_to_send = sign(request_to_send, &sender).unwrap();
        let signature_header = request_to_send.headers().get("signature").unwrap();

        let receive_request = Request::builder()
            .method("POST")
            .uri(uri.path())
            .header("date", HeaderValue::from_str(&date).unwrap())
            .header("host", host)
            .header("digest", digest.to_string())
            .header("signature", signature_header)
            .header(ACCEPT, &SHORT_ACCEPT_HEADER)
            .body(body)
            .unwrap();

        let fields = parse_signature_fields(&receive_request).unwrap();

        assert_eq!(fields.key_id.as_str(), EXPECTED_KEY_ID);

        let public_key_pem = sender.pkey.public_key_to_pem().unwrap();
        let public_key = PKey::public_key_from_pem(public_key_pem.as_slice()).unwrap();

        super::validate_signature(&receive_request, fields, &public_key).unwrap_err();
    }
}
