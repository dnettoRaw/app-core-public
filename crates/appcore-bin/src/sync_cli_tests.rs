// =============================================================================
//        #######
//     ###       ###     F: sync_cli_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{classify_sync_route, read_http_request_parts, SyncRoute};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;

#[test]
fn rejects_sync_request_body_above_limit() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("listener address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let request = "POST /v1/sync/events HTTP/1.1\r\nContent-Length: 1048577\r\n\r\n";
        stream.write_all(request.as_bytes()).expect("write request");
    });
    let mut stream = TcpStream::connect(address).expect("connect client");
    let result = read_http_request_parts(&mut stream);
    server.join().expect("join server");
    assert!(matches!(
        result,
        Err(super::BootstrapError::Runtime(message))
            if message == "sync request body too large"
    ));
}

#[test]
fn removed_sync_route_is_classified_for_the_update_wall() {
    assert_eq!(
        classify_sync_route("POST /sync/events HTTP/1.1\r\n"),
        SyncRoute::Removed
    );
}
