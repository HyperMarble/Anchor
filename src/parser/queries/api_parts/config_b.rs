const CSHARP: LangApiConfig = LangApiConfig {
    check_nodes: &["attribute_list", "invocation_expression"],
    fn_scope: &["method_declaration"],
    class_scope: &["class_declaration"],
    base_path_markers: &["Route("],
    patterns: &[
        // Server: ASP.NET attributes
        ApiPattern::server_on("HttpGet", Some("GET"), &["attribute_list"]),
        ApiPattern::server_on("HttpPost", Some("POST"), &["attribute_list"]),
        ApiPattern::server_on("HttpPut", Some("PUT"), &["attribute_list"]),
        ApiPattern::server_on("HttpDelete", Some("DELETE"), &["attribute_list"]),
        ApiPattern::server_on("HttpPatch", Some("PATCH"), &["attribute_list"]),
        // Server: Minimal API
        ApiPattern::server_on("MapGet(", Some("GET"), &["invocation_expression"]),
        ApiPattern::server_on("MapPost(", Some("POST"), &["invocation_expression"]),
        ApiPattern::server_on("MapPut(", Some("PUT"), &["invocation_expression"]),
        ApiPattern::server_on("MapDelete(", Some("DELETE"), &["invocation_expression"]),
        ApiPattern::server_on("MapPatch(", Some("PATCH"), &["invocation_expression"]),
        // Client: HttpClient
        ApiPattern::client_on("GetAsync(", Some("GET"), &["invocation_expression"]),
        ApiPattern::client_on("PostAsync(", Some("POST"), &["invocation_expression"]),
        ApiPattern::client_on("PutAsync(", Some("PUT"), &["invocation_expression"]),
        ApiPattern::client_on("DeleteAsync(", Some("DELETE"), &["invocation_expression"]),
        ApiPattern::client_on("PatchAsync(", Some("PATCH"), &["invocation_expression"]),
        ApiPattern::client_on("GetStringAsync(", Some("GET"), &["invocation_expression"]),
        ApiPattern::client_on("GetFromJsonAsync(", Some("GET"), &["invocation_expression"]),
        ApiPattern::client_on("PostAsJsonAsync(", Some("POST"), &["invocation_expression"]),
    ],
};

const RUBY: LangApiConfig = LangApiConfig {
    check_nodes: &["call", "method_call"],
    fn_scope: &["method", "singleton_method"],
    class_scope: &["class"],
    base_path_markers: &[],
    patterns: &[
        // Client: HTTP libraries (check before server to avoid .get ambiguity)
        ApiPattern::client_on("HTTParty.get(", Some("GET"), &["method_call"]),
        ApiPattern::client_on("HTTParty.post(", Some("POST"), &["method_call"]),
        ApiPattern::client_on("RestClient.get(", Some("GET"), &["method_call"]),
        ApiPattern::client_on("RestClient.post(", Some("POST"), &["method_call"]),
        ApiPattern::client_on("Faraday.get(", Some("GET"), &["method_call"]),
        ApiPattern::client_on("Faraday.post(", Some("POST"), &["method_call"]),
        ApiPattern::client_on("Typhoeus.get(", Some("GET"), &["method_call"]),
        ApiPattern::client_on("Typhoeus.post(", Some("POST"), &["method_call"]),
        // Server: Rails/Sinatra (standalone calls, no receiver)
        ApiPattern::server_on("get ", Some("GET"), &["call"]),
        ApiPattern::server_on("post ", Some("POST"), &["call"]),
        ApiPattern::server_on("put ", Some("PUT"), &["call"]),
        ApiPattern::server_on("delete ", Some("DELETE"), &["call"]),
        ApiPattern::server_on("patch ", Some("PATCH"), &["call"]),
        ApiPattern::server_on("match ", None, &["call"]),
    ],
};

const RUST: LangApiConfig = LangApiConfig {
    check_nodes: &["attribute_item", "call_expression"],
    fn_scope: &["function_item"],
    class_scope: &[],
    base_path_markers: &[],
    patterns: &[
        // Server: Rocket/Actix attribute macros
        ApiPattern::server_on("#[get(", Some("GET"), &["attribute_item"]),
        ApiPattern::server_on("#[post(", Some("POST"), &["attribute_item"]),
        ApiPattern::server_on("#[put(", Some("PUT"), &["attribute_item"]),
        ApiPattern::server_on("#[delete(", Some("DELETE"), &["attribute_item"]),
        ApiPattern::server_on("#[patch(", Some("PATCH"), &["attribute_item"]),
        ApiPattern::server_on("actix_web::get(", Some("GET"), &["attribute_item"]),
        ApiPattern::server_on("actix_web::post(", Some("POST"), &["attribute_item"]),
        // Server: Axum .route()
        ApiPattern::server_on(".route(", None, &["call_expression"]),
        // Client: reqwest
        ApiPattern::client_on("reqwest::get(", Some("GET"), &["call_expression"]),
        ApiPattern::client_on("reqwest::Client", None, &["call_expression"]),
        ApiPattern::client_on("client.get(", Some("GET"), &["call_expression"]),
        ApiPattern::client_on("client.post(", Some("POST"), &["call_expression"]),
        ApiPattern::client_on("client.put(", Some("PUT"), &["call_expression"]),
        ApiPattern::client_on("client.delete(", Some("DELETE"), &["call_expression"]),
    ],
};

const CPP: LangApiConfig = LangApiConfig {
    check_nodes: &["call_expression"],
    fn_scope: &["function_definition"],
    class_scope: &["class_specifier"],
    base_path_markers: &[],
    patterns: &[
        // Client: cpr library (check before server .Get patterns)
        ApiPattern::client("cpr::Get(", Some("GET")),
        ApiPattern::client("cpr::Post(", Some("POST")),
        ApiPattern::client("cpr::Put(", Some("PUT")),
        ApiPattern::client("cpr::Delete(", Some("DELETE")),
        ApiPattern::client("cpr::Patch(", Some("PATCH")),
        ApiPattern::client("cli.Get(", Some("GET")),
        ApiPattern::client("cli.Post(", Some("POST")),
        ApiPattern::client("client.Get(", Some("GET")),
        ApiPattern::client("client.Post(", Some("POST")),
        // Server: cpp-httplib
        ApiPattern::server("svr.Get(", Some("GET")),
        ApiPattern::server("svr.Post(", Some("POST")),
        ApiPattern::server("svr.Put(", Some("PUT")),
        ApiPattern::server("svr.Delete(", Some("DELETE")),
        ApiPattern::server("server.Get(", Some("GET")),
        ApiPattern::server("server.Post(", Some("POST")),
        // Server: Pistache
        ApiPattern::server("Routes::Get(", Some("GET")),
        ApiPattern::server("Routes::Post(", Some("POST")),
        ApiPattern::server("Routes::Put(", Some("PUT")),
        ApiPattern::server("Routes::Delete(", Some("DELETE")),
        // Server: Crow
        ApiPattern::server("CROW_ROUTE(", None),
    ],
};

const SWIFT: LangApiConfig = LangApiConfig {
    check_nodes: &["call_expression"],
    fn_scope: &["function_declaration"],
    class_scope: &["class_declaration", "struct_declaration"],
    base_path_markers: &[],
    patterns: &[
        // Client: URLSession, Alamofire (check before server .get patterns)
        ApiPattern::client("URLSession", None),
        ApiPattern::client("dataTask(", None),
        ApiPattern::client("URL(string:", None),
        ApiPattern::client("AF.request(", None),
        ApiPattern::client("Alamofire.request(", None),
        // Server: Vapor
        ApiPattern::server("app.get(", Some("GET")),
        ApiPattern::server("app.post(", Some("POST")),
        ApiPattern::server("app.put(", Some("PUT")),
        ApiPattern::server("app.delete(", Some("DELETE")),
        ApiPattern::server("app.patch(", Some("PATCH")),
        ApiPattern::server("router.get(", Some("GET")),
        ApiPattern::server("router.post(", Some("POST")),
    ],
};

// ── Public API ───────────────────────────────────────────────────────────────
