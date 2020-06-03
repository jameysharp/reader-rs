use feed_rs::model::Feed;
use gio::prelude::*;
use glib::MainContext;
use gtk::prelude::*;
use relm::{connect, init, Relm, Update, Widget};
use relm_derive::Msg;
use std::env::args;
use webkit2gtk::{WebView, WebViewExt};

#[derive(Msg)]
enum Action {
    SetFeed(Feed),
    NextPage,
    PreviousPage,
}

struct Model {
    feed: Feed,
    page: usize,
    application: gtk::Application,
}

struct Widgets {
    window: gtk::ApplicationWindow,
    webview: WebView,
    label: gtk::Label,
}

struct Win {
    model: Model,
    widgets: Widgets,
}

impl Update for Win {
    type Model = Model;
    type ModelParam = gtk::Application;
    type Msg = Action;

    fn model(_relm: &Relm<Self>, param: Self::ModelParam) -> Self::Model {
        Model {
            feed: Feed::default(),
            page: 0,
            application: param,
        }
    }

    fn update(&mut self, event: Action) {
        match event {
            Action::SetFeed(feed) => {
                self.model.feed = feed;
                self.model.goto_page(&self.widgets, 0);
            }
            Action::NextPage => {
                let page = self.model.page + 1;
                self.model.goto_page(&self.widgets, page);
            }
            Action::PreviousPage => {
                if let Some(page) = self.model.page.checked_sub(1) {
                    self.model.goto_page(&self.widgets, page);
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

    fn view(relm: &Relm<Self>, model: Self::Model) -> Self {
        let window = gtk::ApplicationWindow::new(&model.application);

        window.set_title("Full-history RSS Reader");
        window.set_position(gtk::WindowPosition::Center);
        window.set_default_size(800, 600);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.add(&vbox);

        let webview = WebView::new();
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

        feedurl.connect_activate({
            let stream = relm.stream().clone();
            move |feedurl| {
                let feedurl = feedurl.clone();
                let stream = stream.clone();
                MainContext::ref_thread_default().spawn_local(async move {
                    // TODO: handle errors
                    let url = feedurl.get_text().expect("feed URL");
                    let body = surf::get(&url).recv_bytes().await.expect(&url);
                    let mut reader = &body[..];
                    let feed = feed_rs::parser::parse(&mut reader).unwrap();
                    for link in feed.links.iter() {
                        if link.rel.as_ref().map_or(false, |rel| rel == "prev-archive") {
                            println!("{:?}", link);
                        }
                    }
                    stream.emit(Action::SetFeed(feed));
                })
            }
        });

        connect!(relm, backbutton, connect_clicked(_), Action::PreviousPage);
        connect!(relm, nextbutton, connect_clicked(_), Action::NextPage);

        // remove right-click context menu items that don't fit in this app's interaction model
        webview.connect_context_menu(|_webview, menu, _event, _hit| {
            use webkit2gtk::ContextMenuAction::*;
            use webkit2gtk::ContextMenuExt;
            use webkit2gtk::ContextMenuItemExt;

            let items = menu.get_items();
            menu.remove_all();
            for item in items {
                match item.get_stock_action() {
                    OpenLink => (),
                    OpenLinkInNewWindow => (),
                    DownloadLinkToDisk => (),
                    OpenImageInNewWindow => (),
                    DownloadImageToDisk => (),
                    OpenFrameInNewWindow => (),
                    GoBack => (),
                    GoForward => (),
                    InspectElement => (),
                    OpenVideoInNewWindow => (),
                    OpenAudioInNewWindow => (),
                    DownloadVideoToDisk => (),
                    DownloadAudioToDisk => (),
                    _ => menu.append(&item),
                }
            }

            false
        });

        webview.connect_decide_policy(|_webview, decision, _type| {
            if let Some(nav) = decision.downcast_ref::<webkit2gtk::NavigationPolicyDecision>() {
                use webkit2gtk::NavigationPolicyDecisionExt;
                use webkit2gtk::NavigationType::*;
                use webkit2gtk::PolicyDecisionExt;
                use webkit2gtk::URIRequestExt;

                match nav.get_navigation_type() {
                    LinkClicked | BackForward => {
                        if let Some(uri) = nav.get_request().and_then(|r| r.get_uri()) {
                            // if opening the link fails, there's nothing useful we can do
                            let _ = gtk::show_uri(None, &uri, 0);
                            decision.ignore();
                        }
                    }
                    _ => (),
                }
            }

            // don't block handling this event, we've done everything we needed to
            false
        });

        Win {
            model,
            widgets: Widgets {
                window,
                webview,
                label,
            },
        }
    }
}

impl Model {
    fn goto_page(&mut self, widgets: &Widgets, page: usize) {
        if let Some(entry) = self.feed.entries.get(page) {
            self.page = page;
            let title = entry.title.as_ref().map_or("", |title| &title.content);
            widgets.label.set_text(title);
            if let Some(body) = entry.content.as_ref().and_then(|c| c.body.as_ref()) {
                widgets.webview.load_html(body, None);
            } else if let Some(link) = entry.links.first() {
                widgets.webview.load_uri(&link.href);
            }
        }
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
