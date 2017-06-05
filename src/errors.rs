use super::hyper;
use super::serde_json;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, Result;
    }

    links {
    }

    foreign_links {
        Http(hyper::Error);
        Json(serde_json::Error);
    }

    errors {
        ApiResponse(t: String) {
            description("Api error response")
            display("Api error response: '{}'", t)
        }
    }
}
