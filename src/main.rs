extern crate gio;
extern crate glib;
extern crate gtk;
extern crate rss;
extern crate webkit2gtk;

use gio::prelude::*;
use gtk::prelude::*;
use rss::Channel;
use rss::extension::ExtensionMap;
use std::cell::Cell;
use std::collections::HashMap;
use std::env::args;
use std::sync::{Arc, Mutex};
use webkit2gtk::{WebContext, WebView, WebViewExt};

struct Entry {
    uri: String,
    title: String,
}

#[derive(Default)]
struct Feed {
    pages: Vec<Entry>,
    page: Cell<usize>,
}

impl Feed {
    fn goto_page(&mut self, label: &gtk::Label, webview: &WebView, page: usize) {
        if let Some(entry) = self.pages.get(page) {
            self.page.set(page);
            webview.load_uri(&entry.uri);
            label.set_text(&entry.title);
        }
    }
}

struct AtomLink {
    href: String,
    mediatype: Option<String>,
    hreflang: Option<String>,
    title: Option<String>,
    length: Option<usize>,
}

fn get_atom_links(namespaces: &HashMap<String, String>, extensions: &ExtensionMap) -> HashMap<String, Vec<AtomLink>> {
    let mut result = HashMap::new();
    let links = namespaces.iter()
        .filter(|&(_, ns)| *ns == "http://www.w3.org/2005/Atom")
        .filter_map(|(qual, _)| extensions.get(qual))
        .filter_map(|ext| ext.get("link"))
        .flat_map(|links| links)
        .map(|link| link.attrs());
    for attrs in links {
        if let Some(href) = attrs.get("href") {
            let rel = attrs.get("rel").cloned().unwrap_or_else(|| "alternate".into());
            result.entry(rel)
                .or_insert_with(|| Vec::new())
                .push(AtomLink {
                    href: href.clone(),
                    mediatype: attrs.get("type").cloned(),
                    hreflang: attrs.get("hreflang").cloned(),
                    title: attrs.get("title").cloned(),
                    length: attrs.get("length").and_then(|v| v.parse().ok()),
                });
        }
    }
    result
}

fn build_ui(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);

    window.set_title("Full-history RSS Reader");
    window.set_position(gtk::WindowPosition::Center);
    window.set_default_size(800, 600);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.add(&vbox);

    let context = WebContext::get_default().unwrap();

    let webview = WebView::new_with_context(&context);
    vbox.pack_end(&webview, true, true, 0);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    controls.set_border_width(5);
    vbox.pack_start(&controls, false, false, 0);

    let feedurl = gtk::Entry::new();
    controls.pack_start(&feedurl, true, true, 0);

    let backbutton = gtk::Button::new_from_icon_name("go-previous", gtk::IconSize::SmallToolbar.into());
    controls.pack_start(&backbutton, false, false, 0);

    let label = gtk::Label::new("");
    controls.pack_start(&label, true, true, 0);

    let nextbutton = gtk::Button::new_from_icon_name("go-next", gtk::IconSize::SmallToolbar.into());
    controls.pack_start(&nextbutton, false, false, 0);

    let feed = Arc::new(Mutex::new(Feed::default()));

    feedurl.set_placeholder_text(Some("Feed URL"));
    feedurl.connect_activate({
        let feed = feed.clone();
        let label = label.clone();
        let webview = webview.clone();
        move |feedurl| {
            let url = feedurl.get_text().expect("feed URL");
            // FIXME: fetch and parse the feed asynchronously
            let channel = Channel::from_url(&url).unwrap();
            let links = get_atom_links(channel.namespaces(), channel.extensions());
            if let Some(archives) = links.get("prev-archive") {
                if archives.len() == 1 {
                    println!("{}", archives[0].href);
                }
            }
            /*{
            let links = channel.namespaces()
                .iter()
                .filter(|&(_, ns)| *ns == "http://www.w3.org/2005/Atom")
                .filter_map(|(qual, _)| channel.extensions().get(qual))
                .filter_map(|ext| ext.get("link"))
                .flat_map(|links| links)
                .map(|link| link.attrs());
            for attrs in links {
                if let (Some(rel), Some(href)) = (attrs.get("rel"), attrs.get("href")) {
                    if *rel == "prev-archive" {
                        println!("{}", href);
                    }
                }
            }
            }*/
            feed.lock().unwrap().pages = channel.into_items()
                .into_iter()
                .filter_map(|item| item.link().map(|link| Entry {
                    uri: link.into(),
                    title: item.title().unwrap_or("").into(),
                }))
                .collect();
            let feed = feed.clone();
            let label = label.clone();
            let webview = webview.clone();
            idle_add(move || {
                feed.lock().unwrap().goto_page(&label, &webview, 0);
                glib::Continue(false)
            });
        }
    });

    backbutton.connect_clicked({
        let feed = feed.clone();
        let label = label.clone();
        let webview = webview.clone();
        move |_| {
            let mut feed = feed.lock().unwrap();
            if let Some(page) = feed.page.get().checked_sub(1) {
                feed.goto_page(&label, &webview, page);
            }
        }
    });

    nextbutton.connect_clicked({
        let feed = feed.clone();
        let label = label.clone();
        let webview = webview.clone();
        move |_| {
            let mut feed = feed.lock().unwrap();
            let page = feed.page.get() + 1;
            feed.goto_page(&label, &webview, page);
        }
    });

    feed.lock().unwrap().goto_page(&label, &webview, 0);

    window.show_all();
}

fn main() {
    let application = gtk::Application::new("net.minilop.reader",
                                            gio::ApplicationFlags::empty())
        .expect("Initialization failed...");

    application.connect_activate(build_ui);
    application.run(&args().collect::<Vec<_>>());
}
