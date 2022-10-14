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
}

impl Context {
    pub fn token(&self) -> Option<Oauth2Token> {
        self.db
            .get("token")
            .expect("should get token from db")
            .map(|s| bincode::deserialize(&s).expect("should deserialize token"))
    }

    pub fn set_token(&self, token: &Oauth2Token) {
        self.db
            .insert(
                "token",
                bincode::serialize(token).expect("should serialize token"),
            )
            .expect("should insert token into db");
    }

    pub fn state(&self) -> Option<CsrfToken> {
        self.db
            .get("state")
            .expect("should get state from db")
            .map(|s| bincode::deserialize(&s).expect("should deserialize state"))
    }

    pub fn set_state(&self, state: &CsrfToken) {
        self.db
            .insert(
                "state",
                bincode::serialize(state).expect("should serialize state"),
            )
            .expect("should insert state into db");
    }

    pub fn verifier(&self) -> Option<PkceCodeVerifier> {
        self.db
            .get("verifier")
            .expect("should get verifier from db")
            .map(|s| bincode::deserialize(&s).expect("should deserialize verifier"))
    }

    pub fn set_verifier(&self, verifier: &PkceCodeVerifier) {
        self.db
            .insert(
                "verifier",
                bincode::serialize(verifier).expect("should serialize verifier"),
            )
            .expect("should insert verifier into db");
    }
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

    ctx.set_state(&state);

    ctx.set_verifier(&verifier);

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
        let ctx = ctx.lock().unwrap();

        let state = ctx
            .state()
            .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "state not found in db"))?;

        if state.secret() != params.state.secret() {
            return Err((StatusCode::BAD_REQUEST, "invalid state").into());
        }

        let client = ctx.client.clone();

        let verifier = ctx.verifier().ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "verifier not found in db",
        ))?;

        (client, verifier)
    };

    // TODO: broken
    let token = client
        .request_token(params.code, verifier)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "failed to request token"))?;

    let ctx = ctx.lock().unwrap();

    ctx.set_token(&token);

    Ok::<_, ErrorResponse>(StatusCode::OK)
}
