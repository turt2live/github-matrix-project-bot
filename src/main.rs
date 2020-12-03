#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use std::time::Duration;

extern crate tokio;

use std::net::SocketAddr;
use hyper::{Server};

// TODO: Should probably use a config at this point
const GH_TOKEN: &str = include_str!("../gh.token");
const GH_USER: &str = include_str!("../gh.username.txt");
const MX_TOKEN: &str = include_str!("../mx.token");
const TEAM_NAME: &str = include_str!("../gh.team.txt"); // just the suffix
const HSURL: &str = include_str!("../hs_base_url.txt");
const ROOM_ID: &str = include_str!("../room_id.txt");

#[derive(Deserialize, Debug, Clone)]
struct GithubSearchResult {
    total_count: i64,
}

#[derive(Debug, Clone)]
struct PendingReviewChecker {
    client: reqwest::Client,
}

impl PendingReviewChecker {
    pub fn new() -> PendingReviewChecker {
        PendingReviewChecker {
            client: reqwest::Client::new(),
        }
    }

    async fn get_review_count(&self) -> Result<i64, Box<dyn std::error::Error + 'static>> {
        let mut base_url: String = "https://api.github.com/search/issues?q=is%3Aopen%20is%3Apr%20team-review-requested%3Amatrix-org%2F".to_owned();

        let mut resp = self.client.get(&(base_url + TEAM_NAME))
            .header(reqwest::header::USER_AGENT, "Github Review Queue 1.1")
            .basic_auth(GH_USER.trim(), Some(GH_TOKEN.trim()))
            .send().await?;

        println!("Status: {}", resp.status());
        //println!("Status: {:#?}", resp.text().await?);

        let mut search: GithubSearchResult = resp.json().await?;

        let matrix_count = search.total_count;

        // idk what I'm doing, so copy/paste for vector-im

        base_url = "https://api.github.com/search/issues?q=is%3Aopen%20is%3Apr%20team-review-requested%3Avector-im%2F".to_owned();
        resp = self.client.get(&(base_url + TEAM_NAME))
            .header(reqwest::header::USER_AGENT, "Github Review Queue 1.1")
            .basic_auth(GH_USER.trim(), Some(GH_TOKEN.trim()))
            .send().await?;

        println!("Status: {}", resp.status());
        //println!("Status: {:#?}", resp.text().await?);

        search = resp.json().await?;

        let vector_count = search.total_count;

        Ok(vector_count + matrix_count)
    }

    async fn update_state(
        &self,
        review_count: i64,
    ) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let severity = if review_count > 0 {
            "warning"
        } else {
            "normal"
        };

        // This feels overly verbose for string concatenation
        let mut req_url: String = "".to_owned();
        req_url.push_str(HSURL);
        req_url.push_str("/_matrix/client/r0/rooms/");
        req_url.push_str(ROOM_ID);
        req_url.push_str("/state/re.jki.counter/");

        self.client.put(&(req_url + "gh_reviews"))
            .header("Authorization", format!("Bearer {}", MX_TOKEN.trim()))
            .json(&json!({
                "title": "Pending reviews",
                "value": review_count,
                "severity": severity,
                "link": "https://github.com/pulls/review-requested",
            }))
            .send().await?;

        Ok(())
    }

    async fn do_check_inner(&self) -> Result<(), Box<dyn std::error::Error + 'static>> {
        let review_count = self.get_review_count().await?;

        println!(
            "There are {} pending reviews",
            review_count,
        );

        self.update_state(review_count).await?;

        Ok(())
    }

    pub async fn do_check(&self) {
        match self.do_check_inner().await {
            Ok(()) => {}
            Err(err) => panic!("Error: {}", err),
        }
    }
}

#[tokio::main]
async fn main() {
    let checker = PendingReviewChecker::new();

    let c = checker.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            c.do_check().await;
            interval.tick().await;
        }
    });

    let make_service = hyper::service::make_service_fn(move |_| {
        let checker = checker.clone();
        async move {
            Ok::<_, hyper::Error>(hyper::service::service_fn(move |_req| {
                let checker = checker.clone();
                async move {
                    checker.do_check().await;
                    Ok::<_, hyper::Error>(hyper::Response::new(hyper::Body::from("Done")))
                }
            }))
        }
    });

    // Then bind and serve...
    let addr = SocketAddr::from(([127, 0, 0, 1], 8089));
    let server = Server::bind(&addr).serve(make_service);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
