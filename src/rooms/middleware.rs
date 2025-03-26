use axum::{
    response::Response,
    http::{
        Request,
        StatusCode,
        header::{COOKIE, SET_COOKIE},
    },
    middleware:: Next,
    body::Body,
};
use uuid::Uuid;

pub async fn ensure_uid(
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {

    // check for cookie
    let has_browser_id = request
        .headers()
        .get(COOKIE)
        .and_then(|cookie| {
            cookie
                .to_str()
                .ok()
                .and_then(|c| c.split(';')
                    .find(|s| s.trim().starts_with("impermachat_id=")))
        })
        .is_some();

    let mut response = next.run(request).await;

    // bestow an ID if none found
    if !has_browser_id {
        let new_browser_id = Uuid::new_v4().to_string();
        let cookie = format!("impermachat_id={}; Path=/; HttpOnly", new_browser_id);
        response.headers_mut().insert(
            SET_COOKIE,
            cookie.parse().unwrap()
        );
    }

    Ok(response)
}
