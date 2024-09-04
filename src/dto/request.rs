use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadSessionRequest {
    /// Entry size in bytes
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadSessionResumeRequest {
    pub session_id: Uuid,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppRolePath {
    pub name: String,
    pub app_id: Uuid,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScopedPermission {
    pub bucket_id: Uuid,
    pub allowance: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModifyRoleRequest {
    pub perms: Vec<ScopedPermission>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberIdRequest {
    pub app_id: Uuid,
    pub id: Uuid,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberRoleRequest {
    pub roles: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenDeleteRequest {
    pub app_id: Uuid,
    pub issuer_id: Uuid,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenListRequest {
    pub app_id: Uuid,
    pub issuer: Option<Uuid>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AddMemberRequest {
    pub app_id: Uuid,
    pub member_id: Uuid,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenIssueRequest {
    pub app_id: Uuid,
    pub name: String,
    pub perms: Vec<ScopedPermission>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RenameEntityRequest {
    pub to: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeleteDirectoryRequest {
    pub recursive: bool,
}
