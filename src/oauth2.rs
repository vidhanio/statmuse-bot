use std::sync::{Arc, Mutex};

use axum::{
    extract::Query,
    http::StatusCode,
    response::{ErrorResponse, IntoResponse, Redirect},
    Extension,
};
use serde::Deserialize;
use sled::Db;
use twitter_v2::{
    authorization::{Oauth2Client, Oauth2Token, Scope},
    oauth2::{AuthorizationCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier},
};

pub struct Context {
    pub client: Oauth2Client,
    pub db: Db,
    pub token: Option<Oauth2Token>,
}

#[allow(clippy::unused_async)]
pub async fn login(Extension(ctx): Extension<Arc<Mutex<Context>>>) -> impl IntoResponse {
    let ctx = ctx.lock().unwrap();

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

    let (url, state) = ctx.client.auth_url(
        challenge,
        [
            Scope::TweetRead,
            Scope::TweetWrite,
            Scope::UsersRead,
            Scope::OfflineAccess,
        ],
    );

    ctx.db
        .insert("state", state.secret().as_str())
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to insert state into db",
            )
        })?;

    ctx.db
        .insert("verifier", verifier.secret().as_str())
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to insert verifier into db",
            )
        })?;

    Ok::<_, ErrorResponse>(Redirect::to(url.as_str()))
}

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: AuthorizationCode,
    pub state: CsrfToken,
}

pub async fn callback(
    Extension(ctx): Extension<Arc<Mutex<Context>>>,
    Query(params): Query<CallbackParams>,
) -> impl IntoResponse {
    let (client, verifier) = {
        let ctx = ctx
            .lock()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "failed to lock ctx"))?;

        let state = ctx
            .db
            .get("state")
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to get state from db",
                )
            })?
            .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "no state found"))?;

        if state != params.state.secret() {
            return Err((StatusCode::BAD_REQUEST, "invalid state").into());
        }

        let client = ctx.client.clone();

        let verifier_bytes = ctx
            .db
            .get("verifier")
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to get verifier from db",
                )
            })?
            .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "no verifier found"))?;

        let verifier =
            PkceCodeVerifier::new(String::from_utf8(verifier_bytes.to_vec()).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to convert verifier to string",
                )
            })?);

        (client, verifier)
    };

    // TODO: broken
    let token = client
        .request_token(params.code, verifier)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "failed to request token"))?;

    let mut ctx = ctx
        .lock()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "failed to lock ctx"))?;

    ctx.token = Some(token);

    Ok::<_, ErrorResponse>(StatusCode::OK)
}
