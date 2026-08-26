use js_runtime::{Context, Runtime};

#[test]
fn url_constructor_parses_simple_url() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com/path?q=1#frag'); \
            return u.protocol + u.hostname + u.pathname + u.search + u.hash; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "https:example.com/path?q=1#frag");
}

#[test]
fn url_constructor_with_base() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('/other', 'https://example.com/path/page.html'); \
            return u.href; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "https://example.com/other");
}

#[test]
fn url_href_setter_reparse() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com/a'); \
            u.href = 'https://other.com:9090/b?q=2#h'; \
            return u.hostname + ':' + u.port + u.pathname + u.search + u.hash; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "other.com:9090/b?q=2#h");
}

#[test]
fn url_origin() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval("new URL('https://example.com:8080/path').origin", "<test>");
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "https://example.com:8080");
}

#[test]
fn url_search_params_get_set() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com?a=1&b=2'); \
            const sp = u.searchParams; \
            return sp.get('a') + ',' + sp.get('b') + ',' + sp.has('c'); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "1,2,false");
}

#[test]
fn url_search_params_append_and_to_string() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com'); \
            u.searchParams.append('key', 'value'); \
            u.searchParams.append('key', 'other'); \
            return u.href + '|' + u.searchParams.toString(); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(
    result.unwrap(),
    "https://example.com/?key=value&key=other|key=value&key=other"
  );
}

#[test]
fn url_search_params_delete_and_set() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com?a=1&b=2&c=3'); \
            u.searchParams.delete('b'); \
            u.searchParams.set('a', '99'); \
            return u.searchParams.toString(); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "c=3&a=99");
}

#[test]
fn url_search_params_has_getall() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
        "(() => { \
            const u = new URL('https://example.com?a=1&a=2&b=3'); \
            return u.searchParams.has('a') + ',' + u.searchParams.has('c') + ',' + u.searchParams.getAll('a').length; \
        })()",
        "<test>",
    );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "true,false,2");
}

#[test]
fn url_search_params_sort() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com'); \
            u.searchParams.append('z', '1'); \
            u.searchParams.append('a', '2'); \
            u.searchParams.sort(); \
            return u.searchParams.toString(); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "a=2&z=1");
}

#[test]
fn url_search_params_entries_keys_values() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com?x=1&y=2'); \
            const keys = u.searchParams.keys(); \
            const vals = u.searchParams.values(); \
            const entries = u.searchParams.entries(); \
            return keys.length + ',' + vals.length + ',' + entries.length; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "2,2,2");
}

#[test]
fn url_search_params_for_each() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com?a=1&b=2'); \
            const seen = []; \
            u.searchParams.forEach((v, k) => { seen.push(k + '=' + v); }); \
            return seen.join(','); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "a=1,b=2");
}

#[test]
fn url_search_params_length() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com?a=1&b=2&c=3'); \
            return u.searchParams.length; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "3");
}

#[test]
fn url_to_string_and_to_json() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com/path?q=1#f'); \
            return (u.toString() === u.href) + ',' + (u.toJSON() === u.href); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "true,true");
}

#[test]
fn url_username_password() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://user:pass@example.com'); \
            return u.username + ',' + u.password; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "user,pass");
}

#[test]
fn url_setters_update_href() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com'); \
            u.pathname = '/new'; \
            return u.href; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "https://example.com/new");
}

#[test]
fn url_hash_setter() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com/path'); \
            u.hash = '#h'; \
            return u.href; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "https://example.com/path#h");
}

#[test]
fn url_search_setter() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com/path'); \
            u.search = '?x=1'; \
            return u.href; \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "https://example.com/path?x=1");
}

#[test]
fn url_invalid_throws_type_error() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "try { new URL('not-a-url'); 'no error' } catch(e) { e.constructor.name }",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "TypeError");
}

#[test]
fn url_constructor_without_args_throws() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "try { new URL(); 'no error' } catch(e) { e.constructor.name }",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "TypeError");
}

#[test]
fn url_search_params_from_string_init() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const sp = new URLSearchParams('a=1&b=2&c=3'); \
            return sp.get('a') + ',' + sp.get('b') + ',' + sp.get('c'); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "1,2,3");
}

#[test]
fn url_search_params_from_array_init() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const sp = new URLSearchParams([['x','10'],['y','20']]); \
            return sp.get('x') + ',' + sp.get('y'); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "10,20");
}

#[test]
fn url_search_params_empty_init() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const sp = new URLSearchParams(); \
            return sp.length + ',' + sp.toString(); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "0,");
}

#[test]
fn url_preserves_search_params_after_href_set() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval(
    "(() => { \
            const u = new URL('https://example.com?a=1'); \
            u.href = 'https://other.com?x=99'; \
            return u.searchParams.get('a') + ',' + u.searchParams.get('x'); \
        })()",
    "<test>",
  );
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "null,99");
}
