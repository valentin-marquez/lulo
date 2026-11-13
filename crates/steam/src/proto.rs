//! The Steam messages this crate sends and reads, written by hand with only the fields we use.
//!
//! Field numbers and wire types must match what Steam's servers speak; unknown fields in
//! replies are skipped by prost, so leaving fields out is safe.

#![allow(missing_docs)]

use prost::Message;

// Connection and logon

#[derive(Clone, PartialEq, Message)]
pub struct CMsgProtoBufHeader {
    #[prost(fixed64, optional, tag = "1")]
    pub steamid: Option<u64>,
    #[prost(int32, optional, tag = "2")]
    pub client_sessionid: Option<i32>,
    #[prost(fixed64, optional, tag = "10")]
    pub jobid_source: Option<u64>,
    #[prost(fixed64, optional, tag = "11")]
    pub jobid_target: Option<u64>,
    #[prost(string, optional, tag = "12")]
    pub target_job_name: Option<String>,
    #[prost(int32, optional, tag = "13")]
    pub eresult: Option<i32>,
    #[prost(string, optional, tag = "14")]
    pub error_message: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgMulti {
    #[prost(uint32, optional, tag = "1")]
    pub size_unzipped: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub message_body: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientHello {
    #[prost(uint32, optional, tag = "1")]
    pub protocol_version: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientHeartBeat {
    #[prost(bool, optional, tag = "1")]
    pub send_reply: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientLogon {
    #[prost(uint32, optional, tag = "1")]
    pub protocol_version: Option<u32>,
    #[prost(uint32, optional, tag = "3")]
    pub cell_id: Option<u32>,
    #[prost(uint32, optional, tag = "7")]
    pub client_os_type: Option<u32>,
    #[prost(bool, optional, tag = "8")]
    pub should_remember_password: Option<bool>,
    #[prost(string, optional, tag = "50")]
    pub account_name: Option<String>,
    #[prost(string, optional, tag = "96")]
    pub machine_name: Option<String>,
    #[prost(bool, optional, tag = "102")]
    pub supports_rate_limit_response: Option<bool>,
    /// Steam expects the refresh token here.
    #[prost(string, optional, tag = "108")]
    pub access_token: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientLogonResponse {
    #[prost(int32, optional, tag = "1")]
    pub eresult: Option<i32>,
    #[prost(int32, optional, tag = "3")]
    pub heartbeat_seconds: Option<i32>,
    #[prost(uint32, optional, tag = "7")]
    pub cell_id: Option<u32>,
}

// Depot keys

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientGetDepotDecryptionKey {
    #[prost(uint32, optional, tag = "1")]
    pub depot_id: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub app_id: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientGetDepotDecryptionKeyResponse {
    #[prost(int32, optional, tag = "1")]
    pub eresult: Option<i32>,
    #[prost(uint32, optional, tag = "2")]
    pub depot_id: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub depot_encryption_key: Option<Vec<u8>>,
}

// Product info (PICS)

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientPicsAccessTokenRequest {
    #[prost(uint32, repeated, packed = "false", tag = "2")]
    pub appids: Vec<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientPicsAccessTokenResponse {
    #[prost(message, repeated, tag = "3")]
    pub app_access_tokens: Vec<AppToken>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppToken {
    #[prost(uint32, optional, tag = "1")]
    pub appid: Option<u32>,
    #[prost(uint64, optional, tag = "2")]
    pub access_token: Option<u64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientPicsProductInfoRequest {
    #[prost(message, repeated, tag = "2")]
    pub apps: Vec<AppRequest>,
    #[prost(bool, optional, tag = "7")]
    pub single_response: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppRequest {
    #[prost(uint32, optional, tag = "1")]
    pub appid: Option<u32>,
    #[prost(uint64, optional, tag = "2")]
    pub access_token: Option<u64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CMsgClientPicsProductInfoResponse {
    #[prost(message, repeated, tag = "1")]
    pub apps: Vec<AppInfo>,
    #[prost(bool, optional, tag = "6")]
    pub response_pending: Option<bool>,
    #[prost(string, optional, tag = "8")]
    pub http_host: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AppInfo {
    #[prost(uint32, optional, tag = "1")]
    pub appid: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "4")]
    pub sha: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "5")]
    pub buffer: Option<Vec<u8>>,
}

// Content servers and manifests

#[derive(Clone, PartialEq, Message)]
pub struct CContentServerDirectoryGetServersForSteamPipeRequest {
    #[prost(uint32, optional, tag = "1")]
    pub cell_id: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CContentServerDirectoryGetServersForSteamPipeResponse {
    #[prost(message, repeated, tag = "1")]
    pub servers: Vec<ServerInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ServerInfo {
    #[prost(string, optional, tag = "8")]
    pub host: Option<String>,
    #[prost(string, optional, tag = "12")]
    pub https_support: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CContentServerDirectoryGetManifestRequestCodeRequest {
    #[prost(uint32, optional, tag = "1")]
    pub app_id: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub depot_id: Option<u32>,
    #[prost(uint64, optional, tag = "3")]
    pub manifest_id: Option<u64>,
    #[prost(string, optional, tag = "4")]
    pub app_branch: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CContentServerDirectoryGetManifestRequestCodeResponse {
    #[prost(uint64, optional, tag = "1")]
    pub manifest_request_code: Option<u64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ContentManifestPayload {
    #[prost(message, repeated, tag = "1")]
    pub mappings: Vec<FileMapping>,
}

#[derive(Clone, PartialEq, Message)]
pub struct FileMapping {
    #[prost(string, optional, tag = "1")]
    pub filename: Option<String>,
    #[prost(uint64, optional, tag = "2")]
    pub size: Option<u64>,
    #[prost(uint32, optional, tag = "3")]
    pub flags: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "5")]
    pub sha_content: Option<Vec<u8>>,
    #[prost(message, repeated, tag = "6")]
    pub chunks: Vec<ChunkData>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ChunkData {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub sha: Option<Vec<u8>>,
    #[prost(fixed32, optional, tag = "2")]
    pub crc: Option<u32>,
    #[prost(uint64, optional, tag = "3")]
    pub offset: Option<u64>,
    #[prost(uint32, optional, tag = "4")]
    pub cb_original: Option<u32>,
    #[prost(uint32, optional, tag = "5")]
    pub cb_compressed: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ContentManifestMetadata {
    #[prost(uint32, optional, tag = "1")]
    pub depot_id: Option<u32>,
    #[prost(uint64, optional, tag = "2")]
    pub gid_manifest: Option<u64>,
    #[prost(bool, optional, tag = "4")]
    pub filenames_encrypted: Option<bool>,
}

// Workshop

#[derive(Clone, PartialEq, Message)]
pub struct CPublishedFileGetDetailsRequest {
    #[prost(fixed64, repeated, packed = "false", tag = "1")]
    pub publishedfileids: Vec<u64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CPublishedFileGetDetailsResponse {
    #[prost(message, repeated, tag = "1")]
    pub publishedfiledetails: Vec<PublishedFileDetails>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PublishedFileDetails {
    #[prost(uint64, optional, tag = "2")]
    pub publishedfileid: Option<u64>,
    #[prost(uint32, optional, tag = "5")]
    pub consumer_appid: Option<u32>,
    #[prost(uint64, optional, tag = "8")]
    pub file_size: Option<u64>,
    #[prost(string, optional, tag = "10")]
    pub file_url: Option<String>,
    #[prost(fixed64, optional, tag = "14")]
    pub hcontent_file: Option<u64>,
    #[prost(string, optional, tag = "16")]
    pub title: Option<String>,
    #[prost(uint32, optional, tag = "20")]
    pub time_updated: Option<u32>,
}

// Authentication (user login)

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationGetPasswordRsaPublicKeyRequest {
    #[prost(string, optional, tag = "1")]
    pub account_name: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationGetPasswordRsaPublicKeyResponse {
    /// Hex.
    #[prost(string, optional, tag = "1")]
    pub publickey_mod: Option<String>,
    /// Hex.
    #[prost(string, optional, tag = "2")]
    pub publickey_exp: Option<String>,
    #[prost(uint64, optional, tag = "3")]
    pub timestamp: Option<u64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationDeviceDetails {
    #[prost(string, optional, tag = "1")]
    pub device_friendly_name: Option<String>,
    #[prost(int32, optional, tag = "2")]
    pub platform_type: Option<i32>,
    #[prost(int32, optional, tag = "3")]
    pub os_type: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationBeginAuthSessionViaQrRequest {
    #[prost(string, optional, tag = "1")]
    pub device_friendly_name: Option<String>,
    #[prost(int32, optional, tag = "2")]
    pub platform_type: Option<i32>,
    #[prost(message, optional, tag = "3")]
    pub device_details: Option<CAuthenticationDeviceDetails>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationBeginAuthSessionViaQrResponse {
    #[prost(uint64, optional, tag = "1")]
    pub client_id: Option<u64>,
    #[prost(string, optional, tag = "2")]
    pub challenge_url: Option<String>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub request_id: Option<Vec<u8>>,
    #[prost(float, optional, tag = "4")]
    pub interval: Option<f32>,
    #[prost(message, repeated, tag = "5")]
    pub allowed_confirmations: Vec<CAuthenticationAllowedConfirmation>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationAllowedConfirmation {
    #[prost(int32, optional, tag = "1")]
    pub confirmation_type: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationBeginAuthSessionViaCredentialsRequest {
    #[prost(string, optional, tag = "1")]
    pub device_friendly_name: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub account_name: Option<String>,
    /// Base64 of the RSA-encrypted password.
    #[prost(string, optional, tag = "3")]
    pub encrypted_password: Option<String>,
    #[prost(uint64, optional, tag = "4")]
    pub encryption_timestamp: Option<u64>,
    #[prost(bool, optional, tag = "5")]
    pub remember_login: Option<bool>,
    #[prost(int32, optional, tag = "6")]
    pub platform_type: Option<i32>,
    #[prost(int32, optional, tag = "7")]
    pub persistence: Option<i32>,
    #[prost(string, optional, tag = "8")]
    pub website_id: Option<String>,
    #[prost(message, optional, tag = "9")]
    pub device_details: Option<CAuthenticationDeviceDetails>,
    #[prost(string, optional, tag = "10")]
    pub guard_data: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationBeginAuthSessionViaCredentialsResponse {
    #[prost(uint64, optional, tag = "1")]
    pub client_id: Option<u64>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub request_id: Option<Vec<u8>>,
    #[prost(float, optional, tag = "3")]
    pub interval: Option<f32>,
    #[prost(message, repeated, tag = "4")]
    pub allowed_confirmations: Vec<CAuthenticationAllowedConfirmation>,
    #[prost(uint64, optional, tag = "5")]
    pub steamid: Option<u64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationPollAuthSessionStatusRequest {
    #[prost(uint64, optional, tag = "1")]
    pub client_id: Option<u64>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub request_id: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationPollAuthSessionStatusResponse {
    #[prost(uint64, optional, tag = "1")]
    pub new_client_id: Option<u64>,
    #[prost(string, optional, tag = "2")]
    pub new_challenge_url: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub refresh_token: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub access_token: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub account_name: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub new_guard_data: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationUpdateAuthSessionWithSteamGuardCodeRequest {
    #[prost(uint64, optional, tag = "1")]
    pub client_id: Option<u64>,
    #[prost(fixed64, optional, tag = "2")]
    pub steamid: Option<u64>,
    #[prost(string, optional, tag = "3")]
    pub code: Option<String>,
    #[prost(int32, optional, tag = "4")]
    pub code_type: Option<i32>,
}

/// The reply carries nothing we need; it only has to decode.
#[derive(Clone, PartialEq, Message)]
pub struct CAuthenticationUpdateAuthSessionWithSteamGuardCodeResponse {}
