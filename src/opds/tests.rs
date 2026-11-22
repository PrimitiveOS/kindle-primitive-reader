use super::*;
use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

const ATOM: &str = r#"<?xml version="1.0"?><a:feed xmlns:a="http://www.w3.org/2005/Atom" xml:base="/base/"><a:title>Harbor &amp; Coast</a:title><a:link rel="next" href="page2" type="application/atom+xml"/><a:link rel="search" href="search.xml" type="application/opensearchdescription+xml"/><!-- feed -->
<a:entry xml:base="books/"><a:title>La<a:b>TeX</a:b> voyage</a:title><a:author><a:name>Ánh Nguyen</a:name></a:author><a:content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><p>First <b>passage</b>.</p><p>Second passage.</p></div></a:content><a:link rel="http://opds-spec.org/acquisition/open-access" href="../files/sea.epub" type="application/epub+zip"/><a:link rel="http://opds-spec.org/acquisition/open-access" href="../files/sea.pdf" type="application/pdf"/></a:entry>
<a:entry><a:title>New books</a:title><a:content><![CDATA[<p>Latest &amp; finest</p>]]></a:content><a:link rel="subsection" href="new" type="application/atom+xml;profile=opds-catalog;kind=acquisition"/></a:entry></a:feed>"#;
fn feed(bytes: &[u8], base: &str) -> Feed {
    let Parsed::Feed(feed) = parse(bytes, base).unwrap() else {
        panic!("Expected feed")
    };
    feed
}

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "reader-opds-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(fs::canonicalize(p).unwrap())
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(crate) struct Server {
    pub(crate) url: String,
    stop: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<String>>>,
    join: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub(crate) fn new(handler: impl Fn(&str, &mut TcpStream) + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = stop.clone();
        let recorded = requests.clone();
        let join = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // BSD accept may inherit the listener's nonblocking mode.
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let mut data = Vec::new();
                        let mut b = [0u8; 1];
                        while data.len() < 16_384 && stream.read(&mut b).unwrap_or(0) == 1 {
                            data.push(b[0]);
                            if data.ends_with(b"\r\n\r\n") {
                                break;
                            }
                        }
                        let request = String::from_utf8_lossy(&data).into_owned();
                        recorded.lock().unwrap().push(request.clone());
                        handler(&request, &mut stream);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(3))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url,
            stop,
            requests,
            join: Some(join),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.join.take().unwrap().join().unwrap();
    }
}
pub(crate) fn response(stream: &mut TcpStream, status: &str, headers: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}
fn link(url: String, kind: &str) -> Link {
    Link {
        url,
        media_type: kind.into(),
        relation: "http://opds-spec.org/acquisition/open-access".into(),
        ..Default::default()
    }
}

#[test]
fn opds_atom_preserves_prefixes_inline_text_cdata_base_and_acquisition_choice() {
    let f = feed(ATOM.as_bytes(), "http://localhost/catalog/root.xml");
    assert_eq!(f.title, "Harbor & Coast");
    assert_eq!(f.entries.len(), 2);
    assert_eq!(f.entries[0].title, "LaTeX voyage");
    assert_eq!(f.entries[0].author, "Ánh Nguyen");
    assert_eq!(f.entries[0].description, "First passage. Second passage.");
    assert_eq!(
        f.entries[0]
            .acquisitions
            .iter()
            .filter_map(Link::extension)
            .collect::<Vec<_>>(),
        ["epub", "pdf"]
    );
    assert_eq!(
        f.entries[0].acquisitions[0].url,
        "http://localhost/base/files/sea.epub"
    );
    assert_eq!(f.next.unwrap().url, "http://localhost/base/page2");
    assert_eq!(f.entries[1].description, "Latest & finest");
    for big_endian in [false, true] {
        let mut bytes = if big_endian {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        for value in ATOM.encode_utf16() {
            bytes.extend_from_slice(&if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            });
        }
        assert_eq!(
            feed(&bytes, "http://localhost/root").entries[0].title,
            "LaTeX voyage"
        );
        bytes.push(0);
        assert!(parse(&bytes, "http://localhost/root").is_err());
    }
}
#[test]
fn opds_json_groups_facets_languages_and_opensearch_templates_are_navigable() {
    let source = serde_json::json!({"metadata":{"title":"Catalog"},"links":[{"rel":["search"],"href":"search{?query}","type":"application/opds+json"},{"rel":"next","href":"?page=2"}],
        "groups":[{"metadata":{"title":"Featured"},"publications":[{"metadata":{"title":{"en":"Sea"},"author":[{"name":"Mira"},"Nam"],"description":"<p>Read <b>here</b></p>"},"links":[{"rel":["download"],"href":"sea.epub","type":"application/epub+zip"}]}]}],
        "facets":[{"metadata":{"title":"Language"},"links":[{"title":"French","href":"/fr","type":"application/opds+json"}]}]});
    let f = feed(
        &serde_json::to_vec(&source).unwrap(),
        "http://localhost/library/catalog",
    );
    assert_eq!(f.entries.len(), 2);
    assert_eq!(f.entries[0].author, "Mira, Nam");
    assert_eq!(f.entries[0].description, "Read here");
    assert_eq!(
        f.entries[1].navigation.as_ref().unwrap().url,
        "http://localhost/fr"
    );
    assert_eq!(
        search_url(&f.search.unwrap().url, "café & sea").unwrap(),
        "http://localhost/library/search?query=caf%C3%A9%20%26%20sea"
    );
    let search = br#"<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/"><Url type="text/html" template="/web?q={searchTerms}"/><Url type="application/atom+xml" template="../find?q={searchTerms}&amp;count={count?}&amp;page={startPage?}"/></OpenSearchDescription>"#;
    let Parsed::Search(template) = parse(search, "http://localhost/catalog/search.xml").unwrap()
    else {
        panic!()
    };
    assert_eq!(
        search_url(&template, "A/B").unwrap(),
        "http://localhost/find?q=A%2FB&count=50&page=1"
    );
    assert!(search_url("http://localhost/{unknown}", "sea").is_err());
    let query_only = model::resolve("http://localhost/catalog?old=1", "{?query}").unwrap();
    assert_eq!(
        search_url(&query_only, "sea").unwrap(),
        "http://localhost/catalog?query=sea"
    );
}
#[test]
fn opds_malformed_untrusted_or_unbounded_metadata_never_becomes_a_catalog() {
    for value in [
        "<html><body>Login</body></html>",
        "<feed xmlns='http://www.w3.org/2005/Atom'>",
        "{\"metadata\":",
        "<!DOCTYPE feed [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><feed xmlns='http://www.w3.org/2005/Atom'><title>&x;</title></feed>",
    ] {
        assert!(parse(value.as_bytes(), "http://localhost/").is_err());
    }
    for url in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "http://user:secret@localhost/",
        "http://localhost/\n",
    ] {
        assert!(checked_url(url).is_err());
    }
    assert!(parse(&vec![b' '; model::MAX_FEED + 1], "http://localhost/").is_err());
    let entries = "<entry><title>A</title></entry>".repeat(513);
    assert!(
        parse(
            format!("<feed xmlns='http://www.w3.org/2005/Atom'>{entries}</feed>").as_bytes(),
            "http://localhost/"
        )
        .is_err()
    );
    let indirect = Link {
        indirect: true,
        ..link("http://localhost/file.epub".into(), "application/epub+zip")
    };
    assert!(indirect.extension().is_none());
    let paid = Link {
        relation: "http://opds-spec.org/acquisition/buy".into(),
        ..link("http://localhost/file.epub".into(), "application/epub+zip")
    };
    assert!(paid.extension().is_none());
}
#[test]
fn opds_redirects_resolve_final_relative_links_and_never_forward_catalog_credentials() {
    let target = Server::new(|_, stream| {
        response(
            stream,
            "200 OK",
            "Content-Type: application/atom+xml\r\n",
            ATOM.as_bytes(),
        )
    });
    let destination = format!("{}/catalog/final", target.url);
    let origin = Server::new(move |request, stream| {
        if !request.contains("Authorization: Basic") && !request.contains("authorization: Basic") {
            response(stream, "401 Unauthorized", "", b"Denied");
        } else {
            response(
                stream,
                "302 Found",
                &format!("Location: {destination}\r\n"),
                b"",
            );
        }
    });
    let auth = Auth {
        origin: origin.url.clone(),
        username: "reader".into(),
        password: "secret".into(),
    };
    assert!(
        run(&Request::Feed {
            url: origin.url.clone(),
            auth: None
        })
        .is_err()
    );
    let Reply::Catalog(Parsed::Feed(f)) = run(&Request::Feed {
        url: origin.url.clone(),
        auth: Some(auth),
    })
    .unwrap() else {
        panic!()
    };
    assert!(f.entries[0].acquisitions[0].url.starts_with(&target.url));
    assert!(origin.requests.lock().unwrap()[1].contains("cmVhZGVyOnNlY3JldA=="));
    assert!(
        target
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| !r.to_ascii_lowercase().contains("authorization:"))
    );
    let looping = Server::new(|_, stream| {
        response(
            stream,
            "307 Temporary Redirect",
            "Location: /again\r\n",
            b"",
        )
    });
    assert!(
        run(&Request::Feed {
            url: looping.url.clone(),
            auth: None
        })
        .is_err()
    );
    assert_eq!(looping.requests.lock().unwrap().len(), 6);
}
#[test]
fn opds_downloads_validate_before_atomic_completion_and_keep_existing_books() {
    let bytes = include_bytes!("../../tests/fixtures/field-notes-epub3.epub").to_vec();
    let served = bytes.clone();
    let server = Server::new(move |request, stream| {
        if request.starts_with("GET /truncated") {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 99999\r\nConnection: close\r\n\r\nshort"
            );
        } else if request.starts_with("GET /login") {
            response(
                stream,
                "200 OK",
                "Content-Type: text/html\r\n",
                b"<html>Login</html>",
            );
        } else {
            response(
                stream,
                "200 OK",
                "Content-Type: application/epub+zip\r\n",
                &served,
            );
        }
    });
    let root = Root::new();
    fs::create_dir(root.0.join("Books")).unwrap();
    let original = root.0.join("Books/Harbor.epub");
    fs::write(&original, b"Existing user publication").unwrap();
    let acquisition = link(format!("{}/book", server.url), "application/epub+zip");
    let temp = temporary(&root.0, Path::new("Books"), "epub").unwrap();
    assert!(matches!(
        run(&Request::Download {
            link: acquisition.clone(),
            auth: None,
            root: root.0.clone(),
            temporary: temp.clone()
        })
        .unwrap(),
        Reply::Downloaded { .. }
    ));
    assert!(!root.0.join("Books/Harbor (2).epub").exists());
    let completed = finalize(&root.0, &temp, "Harbor", &acquisition).unwrap();
    assert_eq!(completed, Path::new("Books/Harbor (2).epub"));
    assert_eq!(fs::read(root.0.join(&completed)).unwrap(), bytes);
    assert_eq!(fs::read(original).unwrap(), b"Existing user publication");
    assert!(crate::Book::open(&root.0.join(completed)).is_ok());
    assert!(!root.0.join(temp).exists());
    for route in ["truncated", "login"] {
        let temp = temporary(&root.0, Path::new("Books"), "epub").unwrap();
        assert!(
            run(&Request::Download {
                link: link(format!("{}/{route}", server.url), "application/epub+zip"),
                auth: None,
                root: root.0.clone(),
                temporary: temp.clone()
            })
            .is_err()
        );
        assert!(!root.0.join(temp).exists());
    }
    let occupied = PathBuf::from("Books/.primitiveos-opds-preserved.part.epub");
    fs::write(root.0.join(&occupied), b"Do not remove").unwrap();
    assert!(
        run(&Request::Download {
            link: acquisition,
            auth: None,
            root: root.0.clone(),
            temporary: occupied.clone()
        })
        .is_err()
    );
    assert_eq!(fs::read(root.0.join(occupied)).unwrap(), b"Do not remove");
}
#[test]
fn opds_destination_paths_collision_names_and_reopened_settings_stay_in_documents() {
    let root = Root::new();
    let outside = Root::new();
    for path in ["../outside", "/tmp", "Books/../../escape", "."] {
        assert!(folder(&root.0, Path::new(path)).is_err());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside.0, root.0.join("Escape")).unwrap();
        assert!(folder(&root.0, Path::new("Escape")).is_err());
    }
    let mut settings = Settings::default();
    settings.catalogs.push(Catalog {
        name: "Local library".into(),
        url: "http://localhost/opds".into(),
        username: "reader".into(),
        password: "secret".into(),
    });
    let path = root.0.join("opds.json");
    settings.save(&path).unwrap();
    let restored = Settings::load(&path);
    assert_eq!(restored.catalogs[0].username, "reader");
    assert_eq!(restored.catalogs[0].password, "secret");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let mut bytes = Vec::new();
    assert!(storage::copy_response(&b"short"[..], &mut bytes, Some(10)).is_err());
    assert!(storage::copy_response(&b""[..], &mut bytes, None).is_err());
    assert!(storage::copy_response(&b"x"[..], &mut bytes, Some(MAX_DOWNLOAD + 1)).is_err());
    let acquisition = link("http://localhost/book.epub".into(), "application/epub+zip");
    let temp = temporary(&root.0, Path::new(""), "epub").unwrap();
    fs::write(
        root.0.join(&temp),
        include_bytes!("../../tests/fixtures/field-notes-epub3.epub"),
    )
    .unwrap();
    let saved = finalize(&root.0, &temp, &"📖".repeat(80), &acquisition).unwrap();
    assert!(saved.file_name().unwrap().as_encoded_bytes().len() <= 255);
    assert!(crate::Book::open(&root.0.join(saved)).is_ok());
    #[cfg(unix)]
    {
        fs::write(outside.0.join("Original.epub"), b"Outside user book").unwrap();
        std::os::unix::fs::symlink(outside.0.join("Original.epub"), root.0.join("Book.epub"))
            .unwrap();
        let temp = temporary(&root.0, Path::new(""), "epub").unwrap();
        fs::write(
            root.0.join(&temp),
            include_bytes!("../../tests/fixtures/field-notes-epub3.epub"),
        )
        .unwrap();
        assert_eq!(
            finalize(&root.0, &temp, "Book", &acquisition).unwrap(),
            Path::new("Book (2).epub")
        );
        assert_eq!(
            fs::read(outside.0.join("Original.epub")).unwrap(),
            b"Outside user book"
        );
    }
}
#[test]
#[ignore = "requires READER_OPDS_WORKER pointing to the freshly built reader-kindle binary"]
fn opds_cancelling_the_actual_child_reaps_it_and_removes_only_its_partial_file() {
    let server = Server::new(|_, stream| {
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\nConnection: close\r\n\r\n"
        );
        for _ in 0..128 {
            if stream.write_all(&[0u8; 8192]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    });
    let root = Root::new();
    fs::write(root.0.join("Original.epub"), b"Keep this").unwrap();
    let temporary = temporary(&root.0, Path::new(""), "epub").unwrap();
    let request = Request::Download {
        link: link(server.url.clone(), "application/epub+zip"),
        auth: None,
        root: root.0.clone(),
        temporary: temporary.clone(),
    };
    let mut command = Command::new(std::env::var_os("READER_OPDS_WORKER").unwrap());
    command.arg("--opds-worker");
    let job = Job::spawn(command, &request).unwrap();
    let pid = job.child.id();
    for _ in 0..200 {
        if fs::metadata(root.0.join(&temporary)).is_ok_and(|m| m.len() > 0) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(root.0.join(&temporary).exists());
    drop(job);
    assert!(!root.0.join(&temporary).exists());
    assert_eq!(
        fs::read(root.0.join("Original.epub")).unwrap(),
        b"Keep this"
    );
    #[cfg(unix)]
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
}
#[test]
#[ignore = "requires READER_OPDS_ORIGINAL pointing to an authorized cached original EPUB"]
fn opds_an_actual_original_download_opens_and_resumes_offline_without_changing_the_source() {
    use primitive_app::App;
    let original = PathBuf::from(std::env::var_os("READER_OPDS_ORIGINAL").unwrap());
    let bytes = fs::read(&original).unwrap();
    let served = bytes.clone();
    let server = Server::new(move |_, stream| {
        response(
            stream,
            "200 OK",
            "Content-Type: application/epub+zip\r\n",
            &served,
        )
    });
    let root = Root::new();
    let acquisition = link(server.url.clone(), "application/epub+zip");
    let temp = temporary(&root.0, Path::new(""), "epub").unwrap();
    run(&Request::Download {
        link: acquisition.clone(),
        auth: None,
        root: root.0.clone(),
        temporary: temp.clone(),
    })
    .unwrap();
    let name = finalize(&root.0, &temp, "Original acquisition", &acquisition).unwrap();
    drop(server);
    assert_eq!(fs::read(root.0.join(&name)).unwrap(), bytes);
    let fonts = vec![crate::test_fixture("fonts/NotoSans-Regular.ttf")];
    let state = root.0.join("reader-state.json");
    let mut reader = crate::app::Reader::open(&root.0, &state, fonts.clone()).unwrap();
    reader.open_path(&root.0.join(&name)).unwrap();
    reader.save().unwrap();
    let mut reopened = crate::app::Reader::open(&root.0, &state, fonts).unwrap();
    reopened.open_path(&root.0.join(&name)).unwrap();
    assert_eq!(fs::read(original).unwrap(), bytes);
}

#[test]
fn opds_plain_text_metadata_and_redirect_boundaries_preserve_their_meaning() {
    let xml = br#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Vectors</title><entry><title>Vector&lt;T&gt; &amp; Map</title><content type="text">Use &lt;T&gt; literally</content></entry></feed>"#;
    let f = feed(xml, "https://catalog.example/root");
    assert_eq!(f.entries[0].title, "Vector<T> & Map");
    assert_eq!(f.entries[0].description, "Use <T> literally");
    let source = serde_json::json!({"metadata":{"title":"Types"},"publications":[{"metadata":{"title":"Vector<T>","description":"<p>Keep &amp;lt; literal &amp; useful</p>"},"links":[{"rel":"download","href":"book.epub","type":"application/epub+zip"}]}]});
    let f = feed(
        &serde_json::to_vec(&source).unwrap(),
        "https://catalog.example/root",
    );
    assert_eq!(f.entries[0].title, "Vector<T>");
    assert_eq!(f.entries[0].description, "Keep &lt; literal & useful");
    let current = Url::parse("https://catalog.example/folder/page").unwrap();
    assert_eq!(
        redirect(&current, "../next").unwrap().as_str(),
        "https://catalog.example/next"
    );
    assert!(redirect(&current, "http://catalog.example/next").is_err());
    assert!(redirect(&current, "file:///etc/passwd").is_err());
    assert!(redirect(&current, "https://user:secret@catalog.example/next").is_err());
}

#[test]
fn opds_chunked_download_and_unavailable_or_broken_feeds_recover_without_publication() {
    let book = include_bytes!("../../tests/fixtures/field-notes-epub3.epub").to_vec();
    let body = book.clone();
    let server = Server::new(move |request, stream| {
        if request.starts_with("GET /chunked") {
            write!(stream,"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/epub+zip\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
            for chunk in body.chunks(1024) {
                write!(stream, "{:x}\r\n", chunk.len()).unwrap();
                stream.write_all(chunk).unwrap();
                stream.write_all(b"\r\n").unwrap();
            }
            stream.write_all(b"0\r\n\r\n").unwrap();
        } else if request.starts_with("GET /broken") {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 100000\r\n\r\n<feed"
            )
            .unwrap();
        } else {
            response(
                stream,
                "200 OK",
                "Content-Type: application/atom+xml\r\n",
                b"<html>Login</html>",
            );
        }
    });
    let root = Root::new();
    let temp = temporary(&root.0, Path::new(""), "epub").unwrap();
    run(&Request::Download {
        link: link(format!("{}/chunked", server.url), "application/epub+zip"),
        auth: None,
        root: root.0.clone(),
        temporary: temp.clone(),
    })
    .unwrap();
    assert_eq!(fs::read(root.0.join(&temp)).unwrap(), book);
    discard(&root.0, &temp);
    for path in ["/broken", "/malformed"] {
        assert!(
            run(&Request::Feed {
                url: format!("{}{path}", server.url),
                auth: None
            })
            .is_err()
        );
    }
    let offline = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", offline.local_addr().unwrap());
    drop(offline);
    assert!(run(&Request::Feed { url, auth: None }).is_err());
    assert!(fs::read_dir(&root.0).unwrap().next().is_none());
}
