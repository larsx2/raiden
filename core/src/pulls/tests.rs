use super::*;

const SEARCH: &str = include_str!("search.json");

fn now() -> u64 {
    unix("2026-11-25T12:00:00Z").expect("a date")
}

fn outcome(code: i32, stdout: &str, stderr: &str) -> Outcome {
    Outcome {
        code: Some(code),
        stdout: stdout.to_owned(),
        stderr: stderr.to_owned(),
        timed_out: false,
    }
}

fn fetched() -> Page {
    let stdout = format!("HTTP/2.0 200 OK\r\nContent-Type: application/json\r\n\r\n{SEARCH}");
    let data = answer(&outcome(0, &stdout, ""), now()).expect("data");
    page(&data).expect("a page")
}

fn listed_in(organizations: &[&str]) -> Vec<Pull> {
    let first = fetched();
    let again = fetched();
    let organizations: Vec<String> = organizations.iter().map(|&name| name.to_owned()).collect();
    finish(
        first.pulls.into_iter().chain(again.pulls).collect(),
        "larsx2",
        &organizations,
    )
}

fn listed() -> Vec<Pull> {
    listed_in(&["fernhill", "tidewater"])
}

fn numbered(pulls: &[Pull], number: u64) -> &Pull {
    pulls
        .iter()
        .find(|pull| pull.number == number)
        .expect("that pull request")
}

fn plain() -> Pull {
    Pull {
        owner: "fernhill".into(),
        repo: "beacon".into(),
        shown: "beacon".into(),
        number: 1,
        title: "A change".into(),
        url: "https://github.com/fernhill/beacon/pull/1".into(),
        draft: false,
        updated_at: now() - 3_600,
        branch: "change".into(),
        base: "main".into(),
        additions: 1,
        deletions: 1,
        files: 1,
        conflicts: false,
        checks: Checks::Passed,
        failed: Vec::new(),
        running: Vec::new(),
        passed: 1,
        failing: false,
        ready: false,
        status: String::new(),
        look: Look::None,
    }
}

#[test]
fn a_recorded_search_becomes_pull_requests() {
    let page = fetched();
    assert_eq!(page.pulls.len(), 6);
    assert_eq!(page.next.as_deref(), Some("Y3Vyc29yOjQw"));
    let retry = numbered(&page.pulls, 169);
    assert_eq!(retry.owner, "fernhill");
    assert_eq!(retry.repo, "beacon");
    assert_eq!(retry.title, "Add a retry budget to the sync worker");
    assert_eq!(retry.branch, "retry-budget");
    assert_eq!(retry.base, "main");
    assert_eq!(
        (retry.additions, retry.deletions, retry.files),
        (228, 45, 27)
    );
    assert_eq!(
        retry.updated_at,
        unix("2026-11-24T10:00:00Z").expect("a date")
    );
    assert_eq!(retry.checks, Checks::Failed);
    assert_eq!(retry.failed, ["test", "e2e"]);
    assert_eq!(retry.running, ["preview/deploy"]);
    assert_eq!(retry.passed, 2);
    assert!(retry.failing && !retry.ready);
    assert_eq!(
        (retry.status.as_str(), retry.look),
        ("Checks failing", Look::Bad)
    );
}

#[test]
fn each_check_rollup_maps_to_a_state_and_status_line() {
    let page = fetched();
    let expect = |number: u64, checks: Checks, status: &str, look: Look| {
        let pull = numbered(&page.pulls, number);
        assert_eq!(pull.checks, checks, "#{number}");
        assert_eq!(pull.status, status, "#{number}");
        assert_eq!(pull.look, look, "#{number}");
    };
    expect(
        12,
        Checks::Failed,
        "Draft, and checks are failing",
        Look::Bad,
    );
    expect(41, Checks::Passed, "Conflicts with develop", Look::Bad);
    expect(7, Checks::None, "No checks ran", Look::None);
    expect(170, Checks::Running, "Checks running", Look::Run);
    expect(
        88,
        Checks::Passed,
        "Ready to merge: checks pass, no conflicts",
        Look::Ok,
    );
    let conflicted = numbered(&page.pulls, 41);
    assert!(conflicted.conflicts && !conflicted.ready);
    assert_eq!(conflicted.passed, 4);
    assert!(!numbered(&page.pulls, 7).conflicts);
    assert_eq!(numbered(&page.pulls, 12).failed, ["ci/legacy"]);
    assert_eq!(numbered(&page.pulls, 170).running, ["test", "build"]);
    assert!(numbered(&page.pulls, 88).ready);
}

#[test]
fn the_first_rule_that_applies_sets_the_status() {
    let settled = |change: fn(&mut Pull)| {
        let mut pull = plain();
        change(&mut pull);
        let pull = pull.settle();
        (pull.status, pull.look, pull.failing, pull.ready)
    };
    assert_eq!(
        settled(|_| {}),
        (
            "Ready to merge: checks pass, no conflicts".into(),
            Look::Ok,
            false,
            true
        )
    );
    assert_eq!(
        settled(|pull| {
            pull.draft = true;
            pull.checks = Checks::Failed;
            pull.conflicts = true;
        }),
        (
            "Draft, and checks are failing".into(),
            Look::Bad,
            true,
            false
        )
    );
    assert_eq!(
        settled(|pull| {
            pull.checks = Checks::Failed;
            pull.conflicts = true;
        }),
        ("Checks failing".into(), Look::Bad, true, false)
    );
    assert_eq!(
        settled(|pull| {
            pull.draft = true;
            pull.conflicts = true;
        }),
        ("Conflicts with main".into(), Look::Bad, false, false)
    );
    assert_eq!(
        settled(|pull| {
            pull.draft = true;
            pull.checks = Checks::Running;
        }),
        ("Draft".into(), Look::None, false, false)
    );
    assert_eq!(
        settled(|pull| pull.checks = Checks::Running),
        ("Checks running".into(), Look::Run, false, false)
    );
    assert_eq!(
        settled(|pull| pull.checks = Checks::None),
        ("No checks ran".into(), Look::None, false, false)
    );
}

#[test]
fn the_list_has_each_pull_request_once_newest_first() {
    let pulls = listed();
    let numbers: Vec<u64> = pulls.iter().map(|pull| pull.number).collect();
    assert_eq!(numbers, [88, 41, 170, 169, 12, 7]);
}

#[test]
fn the_subtitle_drops_the_owner_for_your_repos_and_the_busiest_owner() {
    let pulls = listed();
    let shown = |number| numbered(&pulls, number).shown.as_str();
    assert_eq!(shown(169), "beacon");
    assert_eq!(shown(88), "ledger");
    assert_eq!(shown(12), "notebook");
    assert_eq!(shown(41), "tidewater/lantern");
    assert_eq!(shown(7), "kestrel-io/site");
}

#[test]
fn only_an_organization_you_belong_to_loses_its_owner_in_the_subtitle() {
    let outside = listed_in(&["tidewater"]);
    let shown = |number| numbered(&outside, number).shown.as_str();
    assert_eq!(shown(169), "fernhill/beacon");
    assert_eq!(shown(41), "lantern");
    assert_eq!(shown(12), "notebook");
    assert_eq!(shown(7), "kestrel-io/site");
    let alone = listed_in(&[]);
    let shown = |number| numbered(&alone, number).shown.as_str();
    assert_eq!(shown(169), "fernhill/beacon");
    assert_eq!(shown(41), "tidewater/lantern");
    assert_eq!(shown(12), "notebook");
}

#[test]
fn owners_are_listed_by_open_pull_requests_then_by_name() {
    assert_eq!(
        owners(&listed()),
        [
            Owner {
                name: "fernhill".into(),
                open: 3
            },
            Owner {
                name: "kestrel-io".into(),
                open: 1
            },
            Owner {
                name: "larsx2".into(),
                open: 1
            },
            Owner {
                name: "tidewater".into(),
                open: 1
            },
        ]
    );
}

#[test]
fn the_snapshot_holds_the_tab_newest_first() {
    let pulls = listed();
    let all = snapshot(&pulls, Tab::All, None, None, None, now());
    assert_eq!(all.label, "Open pull requests");
    let lines: Vec<&str> = all.text.lines().collect();
    assert_eq!(lines.len(), 6);
    assert_eq!(
        lines[0],
        "fernhill/ledger#88 Cache the ledger totals (open, checks passed, updated 1h ago)"
    );
    assert_eq!(
        lines[1],
        "tidewater/lantern#41 Move the lantern settings to one file (open, checks passed, conflicts, updated 4h ago)"
    );
    assert_eq!(
        lines[4],
        "larsx2/notebook#12 Draft the release notes page (draft, checks failing: ci/legacy, updated 2d ago)"
    );
    assert_eq!(
        lines[5],
        "kestrel-io/site#7 Bump the font loader (open, no checks, updated 5d ago)"
    );
    let failing = snapshot(
        &pulls,
        Tab::Failing,
        Some("fernhill/beacon"),
        None,
        None,
        now(),
    );
    assert_eq!(failing.label, "Failing pull requests in beacon");
    assert_eq!(
        failing.text,
        "fernhill/beacon#169 Add a retry budget to the sync worker (open, checks failing: test, e2e, updated 1d ago)"
    );
    let ready = snapshot(&pulls, Tab::Ready, Some("fernhill"), None, None, now());
    assert_eq!(ready.label, "Pull requests ready to merge in fernhill");
    assert!(ready.text.starts_with("fernhill/ledger#88 "));
    assert_eq!(ready.text.lines().count(), 1);
    let drafts = snapshot(
        &pulls,
        Tab::Drafts,
        Some("tidewater/lantern"),
        None,
        None,
        now(),
    );
    assert_eq!(drafts.label, "Draft pull requests in tidewater/lantern");
    assert_eq!(drafts.text, "None.");
}

#[test]
fn the_snapshot_holds_only_what_the_typed_filter_matches() {
    let pulls = listed();
    let urls = vec![
        numbered(&pulls, 169).url.clone(),
        numbered(&pulls, 170).url.clone(),
        numbered(&pulls, 41).url.clone(),
    ];
    let matched = snapshot(
        &pulls,
        Tab::All,
        Some("fernhill/beacon"),
        Some(" retry "),
        Some(&urls),
        now(),
    );
    assert_eq!(
        matched.label,
        "Open pull requests in beacon matching “retry”"
    );
    let numbers: Vec<&str> = matched
        .text
        .lines()
        .map(|line| line.split_once(' ').map_or(line, |(repo, _)| repo))
        .collect();
    assert_eq!(numbers, ["fernhill/beacon#170", "fernhill/beacon#169"]);
    let everywhere = snapshot(&pulls, Tab::All, None, Some("retry"), Some(&urls), now());
    assert_eq!(everywhere.label, "Open pull requests matching “retry”");
    assert_eq!(everywhere.text.lines().count(), 3);
    assert!(everywhere.text.starts_with("tidewater/lantern#41 "));
    let failing = snapshot(
        &pulls,
        Tab::Failing,
        None,
        Some("retry"),
        Some(&urls),
        now(),
    );
    assert_eq!(failing.label, "Failing pull requests matching “retry”");
    assert!(
        failing
            .text
            .lines()
            .all(|line| line.contains("checks failing"))
    );
    let nothing = snapshot(&pulls, Tab::All, None, Some("zzz"), Some(&[]), now());
    assert_eq!(nothing.text, "None.");
    let untyped = snapshot(&pulls, Tab::All, None, Some("  "), None, now());
    assert_eq!(untyped.label, "Open pull requests");
    assert_eq!(untyped.text.lines().count(), 6);
}

#[test]
fn the_snapshot_stops_at_60_lines_or_8_kb() {
    let many: Vec<Pull> = (0..100)
        .map(|number| Pull {
            number,
            url: format!("https://github.com/fernhill/beacon/pull/{number}"),
            updated_at: now() - number * 60,
            ..plain()
        })
        .collect();
    let capped = snapshot(&many, Tab::All, None, None, None, now());
    let lines: Vec<&str> = capped.text.lines().collect();
    assert_eq!(lines.len(), 61);
    assert!(lines[0].starts_with("fernhill/beacon#0 "));
    assert_eq!(lines[60], "and 40 more");
    let long: Vec<Pull> = many
        .into_iter()
        .map(|pull| Pull {
            title: "word ".repeat(50),
            ..pull
        })
        .collect();
    let heavy = snapshot(&long, Tab::All, None, None, None, now());
    assert!(heavy.text.len() <= 8 * 1024, "{}", heavy.text.len());
    let kept = heavy.text.lines().count() - 1;
    assert!(kept < 60, "{kept}");
    assert_eq!(
        heavy.text.lines().last(),
        Some(format!("and {} more", 100 - kept).as_str())
    );
}

#[test]
fn only_queries_reach_gh() {
    for query in [
        SEARCH_QUERY.to_owned(),
        viewer_query(0),
        viewer_query(3),
        "{ viewer { login } }".to_owned(),
        "query($q: String!) { search(query: $q, type: ISSUE, first: 1) { issueCount } }".to_owned(),
    ] {
        assert_eq!(read_only(&query), Ok(()), "{query}");
    }
    for query in [
        "mutation { closePullRequest(input: {pullRequestId: \"1\"}) { clientMutationId } }",
        "  Mutation Close { closePullRequest { clientMutationId } }",
        "query { viewer { login } }\nmutation { addStar { clientMutationId } }",
        "subscription { anything }",
        "fragment Pr on PullRequest { title }",
        "",
        "query Q { a(s: \"\"\"x\"y\"\"\") } mutation M { m }",
        "query($q: String = \"mutation\") { viewer { login } }",
        "# a remark\nquery { viewer { login } }",
        "query { viewer { login } } # mutation { m }",
    ] {
        assert!(read_only(query).is_err(), "{query}");
        assert!(arguments(query, &[]).is_err(), "{query}");
    }
}

#[test]
fn gh_is_given_each_field_as_its_own_argument() {
    let fields = vec![
        ("q".to_owned(), search_terms("fernhill", "larsx2")),
        ("after".to_owned(), "@cursor; rm -rf /".to_owned()),
    ];
    let arguments = arguments(SEARCH_QUERY, &fields).expect("a query");
    assert_eq!(arguments[..4], ["api", "graphql", "--include", "-f"]);
    assert_eq!(arguments[4], format!("query={SEARCH_QUERY}"));
    assert_eq!(
        arguments[5..],
        [
            "-f",
            "q=is:pr is:open archived:false org:fernhill",
            "-f",
            "after=@cursor; rm -rf /"
        ]
    );
    assert_eq!(
        search_terms("Larsx2", "larsx2"),
        "is:pr is:open archived:false user:Larsx2"
    );
}

#[test]
fn the_viewer_query_checks_each_pinned_repo() {
    let pinned = vec!["fernhill/ledger".to_owned(), "tidewater/gone".to_owned()];
    let query = viewer_query(pinned.len());
    assert!(query.contains("pin1: repository(owner: $o1, name: $n1)"));
    assert_eq!(
        viewer_fields(&pinned),
        [
            ("reviews".to_owned(), REVIEWS.to_owned()),
            ("o0".to_owned(), "fernhill".to_owned()),
            ("n0".to_owned(), "ledger".to_owned()),
            ("o1".to_owned(), "tidewater".to_owned()),
            ("n1".to_owned(), "gone".to_owned()),
        ]
    );
    let body = r#"{"data":{"viewer":{"login":"larsx2","organizations":{"nodes":[{"login":"fernhill"},{"login":"tidewater"}]}},"reviews":{"issueCount":3},"pin0":{"nameWithOwner":"fernhill/ledger"},"pin1":null},"errors":[{"type":"NOT_FOUND","path":["pin1"],"message":"Could not resolve to a Repository with the name 'tidewater/gone'."}]}"#;
    let stdout = format!("HTTP/2.0 200 OK\n\n{body}");
    let gone = "gh: Could not resolve to a Repository with the name 'tidewater/gone'.";
    let data = answer(&outcome(1, &stdout, gone), now()).expect("partial data");
    assert_eq!(
        viewer(&data, &pinned),
        Ok(Viewer {
            login: "larsx2".into(),
            organizations: vec!["fernhill".into(), "tidewater".into()],
            review_requested: 3,
            lost: vec!["tidewater/gone".into()],
        })
    );
}

#[test]
fn gh_failures_become_plain_problems() {
    let kind = |outcome: Outcome| answer(&outcome, 1_000).expect_err("a problem").kind;
    assert_eq!(
        answer(
            &Outcome {
                timed_out: true,
                ..outcome(-1, "", "")
            },
            1_000
        ),
        Err(Problem::too_long())
    );
    assert_eq!(
        kind(outcome(
            4,
            "",
            "To get started with GitHub CLI, please run:  gh auth login\n"
        )),
        ProblemKind::SignedOut
    );
    assert_eq!(
        kind(outcome(
            1,
            "HTTP/2.0 401 Unauthorized\n\n{\"message\":\"Bad credentials\"}",
            "gh: Bad credentials (HTTP 401)"
        )),
        ProblemKind::SignedOut
    );
    assert_eq!(
        kind(outcome(
            1,
            "",
            "error connecting to api.github.com\ncheck your internet connection or https://githubstatus.com\n"
        )),
        ProblemKind::Offline
    );
    let failed = answer(
        &outcome(
            1,
            "HTTP/2.0 500 Internal Server Error\n\n",
            "gh: Something broke (HTTP 500)\n",
        ),
        1_000,
    )
    .expect_err("a problem");
    assert_eq!(failed.kind, ProblemKind::Failed);
    assert_eq!(
        failed.text,
        "GitHub did not answer as expected: Something broke (HTTP 500)."
    );
    assert_eq!(failed.retry_at, None);
    let unreadable =
        answer(&outcome(0, "HTTP/2.0 200 OK\n\nnot json", ""), 1_000).expect_err("a problem");
    assert_eq!(unreadable.kind, ProblemKind::Failed);
}

#[test]
fn a_rate_limit_says_when_to_try_again() {
    let used_up = outcome(
        1,
        "HTTP/2.0 403 Forbidden\nX-Ratelimit-Remaining: 0\nX-Ratelimit-Reset: 2000\n\n{\"message\":\"API rate limit exceeded for user ID 1.\"}",
        "gh: API rate limit exceeded for user ID 1. (HTTP 403)",
    );
    let problem = answer(&used_up, 1_000).expect_err("limited");
    assert_eq!(problem.kind, ProblemKind::Limited);
    assert_eq!(problem.retry_at, Some(2_000));
    let typed = outcome(
        0,
        "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 0\n\n{\"data\":null,\"errors\":[{\"type\":\"RATE_LIMITED\",\"message\":\"API rate limit exceeded\"}]}",
        "",
    );
    assert_eq!(
        answer(&typed, 1_000).expect_err("limited").retry_at,
        Some(1_060)
    );
    let secondary = outcome(
        1,
        "HTTP/2.0 403 Forbidden\nRetry-After: 30\n\n{\"message\":\"You have exceeded a secondary rate limit.\"}",
        "",
    );
    assert_eq!(
        answer(&secondary, 1_000).expect_err("limited").retry_at,
        Some(1_030)
    );
}

#[test]
fn words_in_titles_are_never_taken_for_gh_failures() {
    let body = r#"{"data":{"search":{"pageInfo":{"hasNextPage":false},"nodes":[{"url":"https://github.com/fernhill/beacon/pull/3","title":"Handle the rate limit, then run gh auth login when not logged in; error connecting to the mirror"}]}}}"#;
    let stdout = format!("HTTP/2.0 200 OK\n\n{body}");
    let data = answer(&outcome(0, &stdout, ""), now()).expect("data");
    assert_eq!(page(&data).expect("a page").pulls.len(), 1);
}

#[test]
fn a_bad_gateway_is_worth_another_try() {
    assert!(retryable(&outcome(
        1,
        "HTTP/2.0 502 Bad Gateway\n\n",
        "gh: HTTP 502"
    )));
    assert!(retryable(&outcome(1, "", "gh: Bad Gateway (HTTP 502)")));
    assert!(!retryable(&outcome(
        1,
        "HTTP/2.0 401 Unauthorized\n\n{}",
        ""
    )));
}

#[test]
fn the_signed_in_account_comes_from_gh_auth_status() {
    let signed_in = outcome(
        0,
        "github.com\n  ✓ Logged in to github.com account larsx2 (keyring)\n  - Active account: true\n",
        "",
    );
    assert_eq!(auth(&signed_in), Ok("larsx2".to_owned()));
    let signed_out = outcome(
        1,
        "",
        "You are not logged into any GitHub hosts. To log in, run: gh auth login\n",
    );
    assert_eq!(auth(&signed_out), Err(Problem::signed_out()));
}

#[test]
fn a_bad_token_in_gh_auth_status_means_signed_out() {
    let bad_token = outcome(
        1,
        "github.com\n  X Failed to log in to github.com account larsx2 (keyring)\n  - Active account: true\n  - The token in keyring is invalid.\n",
        "",
    );
    assert_eq!(auth(&bad_token), Err(Problem::signed_out()));
    let failed_but_exited_cleanly = outcome(
        0,
        "github.com\n  X Failed to log in to github.com account larsx2 (keyring)\n",
        "",
    );
    assert_eq!(auth(&failed_but_exited_cleanly), Err(Problem::signed_out()));
}

#[test]
fn a_logged_in_line_counts_only_when_gh_exits_cleanly() {
    let logged_in = "github.com\n  ✓ Logged in to github.com account larsx2 (keyring)\n";
    assert_eq!(auth(&outcome(0, logged_in, "")), Ok("larsx2".to_owned()));
    assert_eq!(auth(&outcome(1, logged_in, "")), Err(Problem::signed_out()));
    assert_eq!(
        auth(&Outcome {
            timed_out: true,
            ..outcome(-1, logged_in, "")
        }),
        Err(Problem::offline())
    );
}

#[test]
fn a_gh_without_the_active_flag_asks_for_an_update_instead_of_a_sign_in() {
    let old = outcome(
        1,
        "",
        "unknown flag: --active\n\nUsage:  gh auth status [flags]\n",
    );
    let problem = auth(&old).expect_err("an old gh");
    assert_eq!(problem, Problem::old_gh());
    assert_eq!(problem.kind, ProblemKind::Failed);
    assert!(problem.text.contains("brew upgrade gh"));
    assert!(!problem.text.contains("gh auth login"));
}

#[test]
fn the_view_shows_only_lost_repos_still_pinned() {
    let cache = Cache {
        login: Some("larsx2".into()),
        fetched_at: Some(now() - 30),
        review_requested: 2,
        pulls: listed(),
        lost: vec!["tidewater/gone".into(), "fernhill/old".into()],
        problem: None,
    };
    let pinned = vec!["fernhill/ledger".to_owned(), "Tidewater/Gone".to_owned()];
    let view = cache.view(true, &pinned, false);
    assert_eq!(view.lost, ["Tidewater/Gone"]);
    assert_eq!(view.owners[0].name, "fernhill");
    assert!(cache.fresh(now()));
    assert!(!cache.fresh(now() + 60));
    assert!(
        !Cache {
            problem: Some(Problem::offline()),
            ..cache.clone()
        }
        .fresh(now())
    );
    assert!(!Cache::default().fresh(now()));
}

#[test]
fn the_view_speaks_the_screens_names() {
    let view = Cache {
        pulls: vec![plain().settle()],
        problem: Some(Problem::signed_out()),
        ..Cache::default()
    }
    .view(false, &[], true);
    let json = serde_json::to_value(&view).expect("json");
    for key in [
        "allowed",
        "login",
        "pulls",
        "fetchedAt",
        "reviewRequested",
        "owners",
        "pinned",
        "lost",
        "refreshing",
        "problem",
    ] {
        assert!(json.get(key).is_some(), "{key}");
    }
    assert_eq!(json["problem"]["kind"], "signedOut");
    assert!(json["problem"].get("retryAt").is_none());
    assert!(json["login"].is_null() && json["fetchedAt"].is_null());
    let pull = &json["pulls"][0];
    assert_eq!(pull["checks"], "passed");
    assert_eq!(pull["look"], "ok");
    assert_eq!(pull["updatedAt"], now() - 3_600);
    let limited = serde_json::to_value(Problem::limited(5)).expect("json");
    assert_eq!(limited["kind"], "limited");
    assert_eq!(limited["retryAt"], 5);
    assert_eq!(
        serde_json::from_str::<Tab>("\"drafts\"").expect("a tab"),
        Tab::Drafts
    );
}

#[test]
fn pinning_adds_once_and_unpinning_removes() {
    let pinned = vec!["fernhill/ledger".to_owned()];
    assert_eq!(
        pinned_after(&pinned, "tidewater/lantern", true),
        Ok(vec!["fernhill/ledger".into(), "tidewater/lantern".into()])
    );
    assert_eq!(
        pinned_after(&pinned, "Fernhill/Ledger", true),
        Ok(vec!["Fernhill/Ledger".into()])
    );
    assert_eq!(pinned_after(&pinned, "fernhill/ledger", false), Ok(vec![]));
    assert!(pinned_after(&pinned, "fernhill", true).is_err());
}

#[cfg(unix)]
#[test]
fn the_cache_is_private_to_you() {
    use std::os::unix::fs::PermissionsExt as _;

    let state = std::env::temp_dir().join(format!("raiden-pulls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(state.join(FOLDER)).expect("a folder");
    std::fs::set_permissions(state.join(FOLDER), std::fs::Permissions::from_mode(0o755))
        .expect("loose");
    let cache = Cache {
        login: Some("larsx2".into()),
        fetched_at: Some(now()),
        review_requested: 1,
        pulls: listed(),
        lost: Vec::new(),
        problem: None,
    };
    write_cache(&state, &cache).expect("written");
    write_cache(&state, &cache).expect("written again");
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(&state.join(FOLDER)), 0o700);
    assert_eq!(mode(&state.join(FOLDER).join(CACHE)), 0o600);
    let names: Vec<_> = std::fs::read_dir(state.join(FOLDER))
        .expect("the folder")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(names, [CACHE]);
    assert_eq!(read_cache(&state), Some(cache));
    let _ = std::fs::remove_dir_all(&state);
    assert_eq!(read_cache(&state), None);
}

#[test]
fn a_sign_in_check_that_cannot_reach_github_reads_as_offline_not_signed_out() {
    let timed_out = outcome(
        1,
        "",
        "github.com\n  X Timeout trying to log in to github.com account larsx2 (keyring)\n",
    );
    assert_eq!(auth(&timed_out), Err(Problem::offline()));
    let no_network = outcome(1, "", "error connecting to api.github.com\n");
    assert_eq!(auth(&no_network), Err(Problem::offline()));
}
