use emoji_printer::print_emojis;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let user_id = env::var("DAUERZUSAGE_ID")
        .map_err(|e| format!("Could not read environment variable DAUERZUSAGE_ID: {e}"))?;
    let user_mail = env::var("DAUERZUSAGE_EMAIL")
        .map_err(|e| format!("Could not read environment variable DAUERZUSAGE_EMAIL: {e}"))?;
    let user_password = env::var("DAUERZUSAGE_PASSWORT")
        .map_err(|e| format!("Could not read environment variable DAUERZUSAGE_PASSWORT: {e}"))?;

    let url = "https://www.spielerplus.de/events";

    log::info!("Fetching {:?}...", url);

    let client = reqwest::blocking::Client::builder()
        .cookie_store(true)
        .build()?;

    let res = client.get(url).send()?;
    log::info!("/events response: {:?} {}", res.version(), res.status());

    let text = res.text()?;

    let mut document_events = scraper::Html::parse_document(&text);

    let title_selector = scraper::Selector::parse("title").unwrap();

    let mut titles = document_events
        .select(&title_selector)
        .map(|x| x.inner_html());

    //log::info!("{:?}", titles.next().map(|x| x.inner_html()));
    let mut title = titles.next().ok_or("missing title")?;
    if title == "Einloggen" {
        log::info!("Login required");
        let csrf_token_selector =
            scraper::Selector::parse("form#login-form input[name=\"_csrf\"]").unwrap();
        let csrf_token_element = document_events
            .select(&csrf_token_selector)
            .next()
            .ok_or("missing csrf token")?;
        let csrf_token = csrf_token_element
            .value()
            .attr("value")
            .ok_or("missing value in csrf token")?;
        let res = client
            .post("https://www.spielerplus.de/site/login")
            .form(&[
                ("_csrf", csrf_token),
                ("LoginForm[email]", &user_mail),
                ("LoginForm[password]", &user_password),
            ])
            .send()?;

        log::info!("/site/login response: {:?} {}", res.version(), res.status());

        let text = res.text()?;
        let document = scraper::Html::parse_document(&text);
        let mut titles = document.select(&title_selector).map(|x| x.inner_html());
        if let Some(title) = titles.next() {
            if title == "Team auswählen" || title == "Select team" {
                // alright!
            } else {
                return Err(format!(
                    "title is not 'Team auswählen' or 'Select team', but '{}'",
                    title
                )
                .into());
            }
        } else {
            return Err("title missing".into());
        }

        let res = client
            .get(format!(
                "https://www.spielerplus.de/site/switch-user?id={user_id}"
            ))
            .send()?;
        log::info!(
            "/site/switch-user response: {:?} {}",
            res.version(),
            res.status()
        );

        let res = client.get(url).send()?;
        log::info!("/events response: {:?} {}", res.version(), res.status());
        let text = res.text()?;
        document_events = scraper::Html::parse_document(&text);
        let mut titles = document_events
            .select(&title_selector)
            .map(|x| x.inner_html());
        title = titles.next().ok_or("missing title")?;
    }
    if title != "Termine" && title != "Events" {
        return Err(format!("title is not 'Termine' or 'Events', but '{}'", title).into());
    }

    // TODO parse trainings
    //log::info!("{:?}", document_events);
    let event_selector = scraper::Selector::parse(".event").unwrap();
    let panel_selector = scraper::Selector::parse(".panel").unwrap();
    let panel_heading_text_selector = scraper::Selector::parse(".panel-heading-text").unwrap();
    let panel_heading_info_selector = scraper::Selector::parse(".panel-heading-info").unwrap();
    let panel_title_selector = scraper::Selector::parse(".panel-title").unwrap();
    let panel_subtitle_selector = scraper::Selector::parse(".panel-subtitle").unwrap();
    let participation_widget_buttons_selector =
        scraper::Selector::parse(".participation-widget-buttons").unwrap();
    let selected_selector = scraper::Selector::parse(".selected").unwrap();
    let events: Vec<_> = document_events.select(&event_selector).collect();
    if events.len() == 0 {
        return Err("No events found".into());
    }
    for event in events {
        log::debug!("Handling event {}", event.inner_html());
        let panel = event
            .select(&panel_selector)
            .next()
            .ok_or("missing .panel")?;
        let heading_text = event
            .select(&panel_heading_text_selector)
            .next()
            .ok_or("missing .panel-heading-text")?;
        let event_title_html = heading_text
            .select(&panel_title_selector)
            .next()
            .ok_or("missing .panel-title")?
            .inner_html();
        let heading_info = event
            .select(&panel_heading_info_selector)
            .next()
            .ok_or("missing .panel-heading-info")?;
        let event_weekday_html = heading_info
            .select(&panel_title_selector)
            .next()
            .ok_or("missing .panel-title")?
            .inner_html();
        let event_date_html = heading_info
            .select(&panel_subtitle_selector)
            .next()
            .ok_or("missing .panel-subtitle")?
            .inner_html();
        let widget_buttons = event
            .select(&participation_widget_buttons_selector)
            .next()
            .ok_or("missing .participation-widget-buttons")?;

        let event_id_parts = panel
            .value()
            .id()
            .ok_or("no panel id found")?
            .split("-")
            .collect::<Vec<_>>();

        let event_kind = *event_id_parts
            .get(1)
            .ok_or("panel id is misformed (no event on index 1)")?;

        if event_kind != "training" {
            continue;
        }

        let zusage = match widget_buttons.select(&selected_selector).next() {
            Some(selected_button) => {
                match selected_button
                    .value()
                    .attr("title")
                    .ok_or("missing selected_button attr 'title'")?
                {
                    "Zugesagt" | "Confirmed" => print_emojis(":check_mark:"),
                    "Unsicher" | "Unsure" => print_emojis(":red_question_mark:"),
                    "Absagen / Abwesend" | "Declined / Absent" => print_emojis(":cross_mark:"),
                    other => {
                        return Err(
                            format!("Unknown selected Zusage button title '{other}'").into()
                        );
                    }
                }
            }
            None => {
                let training_id = panel
                    .value()
                    .attr("id")
                    .ok_or("missing panel attr 'id'")?
                    .strip_prefix("event-training-")
                    .ok_or("panel attr 'id' does not start with 'event-training-'")?;
                log::info!("No Zusage yet for Training {training_id}");
                let res = client
                    .post("https://www.spielerplus.de/events/ajax-participation-form")
                    .form(&[
                        ("Participation[participation]", "1"),
                        ("Participation[reason]", "Dauerzusage"),
                        ("Participation[type]", "training"),
                        ("Participation[typeid]", training_id),
                        ("Participation[user_id]", &user_id),
                    ])
                    .send()?;
                log::info!(
                    "/events/ajax-participation-form response: {:?} {}",
                    res.version(),
                    res.status()
                );
                if res.status() != reqwest::StatusCode::from_u16(200).unwrap() {
                    return Err(
                        "/events/ajax-participation-form response status is not '200 OK'".into(),
                    );
                }
                print_emojis(":pencil:")
            }
        };
        println!(
            "{} {} {} {}",
            zusage, event_weekday_html, event_date_html, event_title_html
        );
    }

    //std::thread::sleep(std::time::Duration::from_secs(2));
    println!("Press return to exit");
    use std::io::BufRead;
    std::io::stdin()
        .lock()
        .lines()
        .next()
        .expect("there was no next line")
        .expect("the line could not be read");

    Ok(())
}
