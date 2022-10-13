use std::sync::{Arc, Mutex};

use axum::{
    extract::Query,
    http::StatusCode,
    response::{ErrorResponse, IntoResponse, Redirect},
    Extension,
};
use serde::Deserialize;
use twitter_v2::{
    authorization::{Oauth2Client, Oauth2Token, Scope},
    oauth2::{AuthorizationCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier},
};

pub struct Context {
    pub client: Oauth2Client,
    pub verifier: Option<PkceCodeVerifier>,
    pub state: Option<CsrfToken>,
    pub token: Option<Oauth2Token>,
}

#[allow(clippy::unused_async)]
pub async fn login(Extension(ctx): Extension<Arc<Mutex<Context>>>) -> impl IntoResponse {
    let mut ctx = ctx.lock().unwrap();

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

    let (url, state) = ctx
        .client
        .auth_url(challenge, [Scope::TweetRead, Scope::TweetWrite]);

    ctx.verifier = Some(verifier);
    ctx.state = Some(state);

    Redirect::to(url.as_str())
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
        let mut ctx = ctx
            .lock()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "failed to lock ctx"))?;

        let state = ctx
            .state
            .take()
            .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "no state found"))?;

        if state.secret() != params.state.secret() {
            return Err((StatusCode::BAD_REQUEST, "invalid state").into());
        }

        let client = Oauth2Client::clone(&ctx.client);

        let verifier = ctx
            .verifier
            .take()
            .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "no verifier found"))?;

        (client, verifier)
    };

    let token = client
        .request_token(params.code, verifier)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "failed to request token"))?;

    let mut ctx = ctx
        .lock()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "failed to lock ctx"))?;

    ctx.token = Some(token);

    Ok::<_, ErrorResponse>(Redirect::to("https://twitter.com"))
}
