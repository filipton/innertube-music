//! Asks the real YouTube Music, for checking a change against today's answers:
//! `cargo run --example live -- search <query>`, `artist <UC...>`, `album <MPREb_...>` or `home`.

use std::future::Future;
use std::task::{Context, Poll, Waker};

use innertube_music::{Http, InnerTube, Item, Request, Response};

/// A blocking ureq client behind ready futures.
struct Live;

impl Http for Live {
    type Error = String;

    fn send(&self, r: Request) -> impl Future<Output = Result<Response, String>> + Send {
        std::future::ready(send(r))
    }
}

fn send(r: Request) -> Result<Response, String> {
    let mut call = if r.body.is_some() { ureq::post(&r.url) } else { ureq::get(&r.url) };
    for (name, value) in &r.headers {
        call = call.set(name, value);
    }
    let answer = match &r.body {
        Some(body) => call.set("Content-Type", "application/json").send_string(body),
        None => call.call(),
    };
    let answer = match answer {
        Ok(a) | Err(ureq::Error::Status(_, a)) => a,
        Err(e) => return Err(e.to_string()),
    };
    let status = answer.status();
    let set_cookies = answer.all("set-cookie").into_iter().map(String::from).collect();
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut answer.into_reader(), &mut body).map_err(|e| e.to_string())?;
    Ok(Response { status, body, set_cookies })
}

/// Runs `f` to its end; every request answers at once, so it never waits.
fn run<T>(f: impl Future<Output = T>) -> T {
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (what, rest) = args.split_first().map(|(w, r)| (w.as_str(), r.join(" "))).unwrap_or(("", String::new()));
    let yt = InnerTube::default();
    match what {
        "search" => {
            let found = run(yt.search(&Live, &rest)).expect("search");
            println!("songs:");
            for s in &found.songs {
                println!("  {} — {}{}", s.title, s.artist, if s.video { " (video)" } else { "" });
            }
            println!("albums:");
            for a in &found.albums {
                println!("  {} — {} ({:?} {})", a.title, a.artist, a.release_type, a.year);
            }
            println!("artists:");
            for a in &found.artists {
                println!("  {} {} {}", a.name, a.id, if a.thumbnail.is_some() { "pictured" } else { "" });
            }
        }
        "artist" => {
            let page = run(yt.artist(&Live, &rest)).expect("artist");
            println!("{} {:?}", page.artist.name, page.artist.thumbnail.map(|t| innertube_music::thumbnail_url(&t, 800)));
            for a in &page.albums {
                println!("  {} ({:?} {})", a.title, a.release_type, a.year);
            }
            println!("{} top songs, {} similar", page.top_songs.len(), page.similar.len());
        }
        "album" => {
            let page = run(yt.album(&Live, &rest)).expect("album");
            println!("{} — {} ({:?} {})", page.album.title, page.album.artist, page.album.release_type, page.album.year);
            for s in &page.songs {
                println!("  {}. {} {}s", s.track, s.title, s.duration);
            }
        }
        "home" => {
            for shelf in run(yt.home(&Live)).expect("home") {
                let kinds = shelf.items.iter().map(|i| match i {
                    Item::Song(s) if s.video => 'v',
                    Item::Song(_) => 's',
                    Item::Album(_) => 'a',
                    Item::Artist(_) => 'r',
                    Item::Playlist(_) => 'p',
                });
                println!("{} [{}]", shelf.title, kinds.collect::<String>());
            }
        }
        _ => eprintln!("usage: live search <query> | artist <id> | album <id> | home"),
    }
}
