use gio::prelude::*;
use glib::GString;
use gtk::prelude::*;
use relm::{connect, init, Relm, Update, Widget};
use relm_derive::Msg;
use rss::extension::ExtensionMap;
use rss::Channel;
use std::collections::HashMap;
use std::env::args;
use webkit2gtk::{WebContext, WebView, WebViewExt};

#[derive(Msg, Debug)]
pub enum Action {
    LoadFeed(GString),
    NextPage,
    PreviousPage,
}

struct Entry {
    uri: String,
    title: String,
}

struct Feed {
    pages: Vec<Entry>,
    page: usize,
    application: gtk::Application,
}

struct AtomLink {
    href: String,
    mediatype: Option<String>,
    hreflang: Option<String>,
    title: Option<String>,
    length: Option<usize>,
}

fn get_atom_links(
    namespaces: &HashMap<String, String>,
    extensions: &ExtensionMap,
) -> HashMap<String, Vec<AtomLink>> {
    let mut result = HashMap::new();
    let links = namespaces
        .iter()
        .filter(|&(_, ns)| *ns == "http://www.w3.org/2005/Atom")
        .filter_map(|(qual, _)| extensions.get(qual))
        .filter_map(|ext| ext.get("link"))
        .flat_map(|links| links)
        .map(|link| link.attrs());
    for attrs in links {
        if let Some(href) = attrs.get("href") {
            let rel = attrs
                .get("rel")
                .cloned()
                .unwrap_or_else(|| "alternate".into());
            result
                .entry(rel)
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

struct Widgets {
    window: gtk::ApplicationWindow,
    webview: WebView,
    label: gtk::Label,
}

struct Win {
    feed: Feed,
    widgets: Widgets,
}

impl Update for Win {
    type Model = Feed;
    type ModelParam = gtk::Application;
    type Msg = Action;

    fn model(_relm: &Relm<Self>, param: Self::ModelParam) -> Self::Model {
        Feed {
            pages: Vec::new(),
            page: 0,
            application: param,
        }
    }

    fn update(&mut self, event: Action) {
        match event {
            Action::LoadFeed(url) => {
                self.feed.load(url);
                self.feed.goto_page(&self.widgets, 0);
            }
            Action::NextPage => {
                let page = self.feed.page + 1;
                self.feed.goto_page(&self.widgets, page);
            }
            Action::PreviousPage => {
                if let Some(page) = self.feed.page.checked_sub(1) {
                    self.feed.goto_page(&self.widgets, page);
                }
            }
        }
    }
}

impl Widget for Win {
    type Root = gtk::ApplicationWindow;

    fn root(&self) -> Self::Root {
        self.widgets.window.clone()
    }

    fn view(relm: &Relm<Self>, feed: Self::Model) -> Self {
        let window = gtk::ApplicationWindow::new(&feed.application);

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
        feedurl.set_placeholder_text(Some("Feed URL"));
        controls.pack_start(&feedurl, true, true, 0);

        let backbutton = gtk::Button::new_from_icon_name(
            Some("go-previous"),
            gtk::IconSize::SmallToolbar.into(),
        );
        controls.pack_start(&backbutton, false, false, 0);

        let label = gtk::Label::new(None);
        controls.pack_start(&label, true, true, 0);

        let nextbutton =
            gtk::Button::new_from_icon_name(Some("go-next"), gtk::IconSize::SmallToolbar.into());
        controls.pack_start(&nextbutton, false, false, 0);

        window.show_all();

        connect!(
            relm,
            feedurl,
            connect_activate(feedurl),
            Action::LoadFeed(feedurl.get_text().expect("feed URL"))
        );
        connect!(relm, backbutton, connect_clicked(_), Action::PreviousPage);
        connect!(relm, nextbutton, connect_clicked(_), Action::NextPage);

        Win {
            feed,
            widgets: Widgets {
                window,
                webview,
                label,
            },
        }
    }
}

impl Feed {
    fn goto_page(&mut self, widgets: &Widgets, page: usize) {
        if let Some(entry) = self.pages.get(page) {
            self.page = page;
            widgets.webview.load_uri(&entry.uri);
            widgets.label.set_text(&entry.title);
        }
    }

    fn load(&mut self, url: GString) {
        // FIXME: fetch and parse the feed asynchronously
        let channel = Channel::from_url(&url).unwrap();
        let links = get_atom_links(channel.namespaces(), channel.extensions());
        if let Some(archives) = links.get("prev-archive") {
            if archives.len() == 1 {
                println!("{}", archives[0].href);
            }
        }
        self.pages = channel
            .into_items()
            .into_iter()
            .filter_map(|item| {
                item.link().map(|link| Entry {
                    uri: link.into(),
                    title: item.title().unwrap_or("").into(),
                })
            })
            .collect();
    }
}

fn main() {
    let application =
        gtk::Application::new(Some("net.minilop.reader"), gio::ApplicationFlags::empty())
            .expect("Initialization failed...");

    application.connect_activate(|application| {
        std::mem::forget(init::<Win>(application.clone()).expect("window init"));
    });
    application.run(&args().collect::<Vec<_>>());
}
