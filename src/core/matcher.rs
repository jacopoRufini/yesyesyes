use super::Rule;

/// One element of a platform's accessibility tree (macOS AX, Windows UI Automation, Linux AT-SPI).
pub trait Node: Sized {
    fn is_button(&self) -> bool;
    /// Title, description, value: whatever text the element shows.
    fn labels(&self) -> Vec<String>;
    fn children(&self) -> Vec<Self>;
    /// Labels of actions the element offers without a button (macOS banners), e.g. "Allow once".
    fn named_actions(&self) -> Vec<String>;
    fn press(&self) -> bool;
    fn perform_named(&self, label: &str) -> bool;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Outcome {
    Pressed,
    Failed,
    /// Matched but left alone (`find_all`).
    Found,
    NotFound,
}

/// What happened, and the text of the notification (read before pressing: it goes away after).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Hit {
    pub outcome: Outcome,
    pub texts: Vec<String>,
}

impl Hit {
    fn not_found() -> Self {
        Hit { outcome: Outcome::NotFound, texts: Vec::new() }
    }
}

/// Presses `rule.action` inside the smallest element whose text mentions `rule.app`.
pub fn press<N: Node>(root: &N, rule: &Rule) -> Hit {
    matching(root, rule)
}

/// Every notification `press` would press, one after the other, without pressing (`Outcome::Found`).
pub fn find_all<N: Node>(root: &N, rule: &Rule) -> Vec<Hit> {
    let mut hits = Vec::new();
    if rule.is_valid() {
        collect(root, &rule.app.to_lowercase(), &rule.action.to_lowercase(), &mut hits);
    }
    hits
}

/// Same choice as `visit`: the innermost element that mentions the app and has the button.
fn collect<N: Node>(node: &N, app: &str, action: &str, hits: &mut Vec<Hit>) -> bool {
    let mut mentioned = mentions(&node.labels(), app);
    let before = hits.len();
    for child in node.children() {
        mentioned |= collect(&child, app, action, hits);
    }
    if hits.len() > before || !mentioned {
        return mentioned;
    }
    if find_button(node, action).is_some() || node.named_actions().iter().any(|l| l.to_lowercase() == action) {
        hits.push(Hit { outcome: Outcome::Found, texts: texts(node) });
    }
    true
}

fn matching<N: Node>(root: &N, rule: &Rule) -> Hit {
    if !rule.is_valid() {
        return Hit::not_found();
    }
    let app = rule.app.to_lowercase();
    let action = rule.action.to_lowercase();
    visit(root, &app, &action).0
}

fn mentions(labels: &[String], app: &str) -> bool {
    labels.iter().any(|l| l.to_lowercase().contains(app))
}

/// Returns the hit and whether the subtree mentions the app.
fn visit<N: Node>(node: &N, app: &str, action: &str) -> (Hit, bool) {
    let mut mentioned = mentions(&node.labels(), app);
    for child in node.children() {
        let (hit, child_mentions) = visit(&child, app, action);
        if hit.outcome != Outcome::NotFound {
            return (hit, true);
        }
        mentioned |= child_mentions;
    }
    if !mentioned {
        return (Hit::not_found(), false);
    }
    if let Some(button) = find_button(node, action) {
        let texts = texts(node);
        return (Hit { outcome: result(button.press()), texts }, true);
    }
    if let Some(label) = node.named_actions().into_iter().find(|l| l.to_lowercase() == action) {
        let texts = texts(node);
        return (Hit { outcome: result(node.perform_named(&label)), texts }, true);
    }
    (Hit::not_found(), true)
}

/// The lines of a notification: its own description (on macOS the whole text) and the text of its
/// innermost elements (buttons left out). The visible lines are often cut ("…"), so a line that is
/// part of a longer one is dropped.
fn texts<N: Node>(node: &N) -> Vec<String> {
    let mut lines = node.labels();
    collect_leaf_texts(node, &mut lines);
    let lines: Vec<String> = lines.into_iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    let mut kept: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let core = uncut(line);
        let covered = lines.iter().enumerate().any(|(j, other)| {
            j != i && other.contains(core) && (other.len() > line.len() || (other == line && j < i))
        });
        if !covered {
            kept.push(line.clone());
        }
    }
    kept
}

/// A line without the ellipsis macOS adds when it cuts the text to fit.
fn uncut(line: &str) -> &str {
    line.trim_end_matches(['…', '.']).trim_end()
}

fn collect_leaf_texts<N: Node>(node: &N, lines: &mut Vec<String>) {
    let children = node.children();
    if children.is_empty() {
        if !node.is_button() {
            lines.extend(node.labels());
        }
        return;
    }
    for child in &children {
        collect_leaf_texts(child, lines);
    }
}

fn find_button<N: Node>(node: &N, action: &str) -> Option<N> {
    for child in node.children() {
        if child.is_button() && child.labels().iter().any(|l| l.to_lowercase() == action) {
            return Some(child);
        }
        if let Some(found) = find_button(&child, action) {
            return Some(found);
        }
    }
    None
}

fn result(ok: bool) -> Outcome {
    if ok { Outcome::Pressed } else { Outcome::Failed }
}

#[cfg(test)]
pub mod fake {
    use super::Node;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A tree in memory; `performed` is shared by clones so tests can look inside.
    #[derive(Clone, Default)]
    pub struct FakeNode {
        pub button: bool,
        pub labels: Vec<String>,
        pub actions: Vec<String>,
        pub children: Vec<FakeNode>,
        pub fails: bool,
        pub performed: Rc<RefCell<Vec<String>>>,
    }

    impl FakeNode {
        pub fn text(s: &str) -> Self {
            FakeNode { labels: vec![s.into()], ..Default::default() }
        }
        pub fn button(s: &str) -> Self {
            FakeNode { button: true, labels: vec![s.into()], ..Default::default() }
        }
        pub fn group(children: Vec<FakeNode>) -> Self {
            FakeNode { children, ..Default::default() }
        }
        /// A macOS-style banner: the actions belong to the banner itself.
        pub fn banner(app: &str, actions: &[&str]) -> Self {
            FakeNode {
                labels: vec![format!("{app}, run it?")],
                actions: actions.iter().map(|a| a.to_string()).collect(),
                children: vec![FakeNode::text(app)],
                ..Default::default()
            }
        }
        pub fn performed(&self) -> Vec<String> {
            self.performed.borrow().clone()
        }
    }

    impl Node for FakeNode {
        fn is_button(&self) -> bool {
            self.button
        }
        fn labels(&self) -> Vec<String> {
            self.labels.clone()
        }
        fn children(&self) -> Vec<Self> {
            self.children.clone()
        }
        fn named_actions(&self) -> Vec<String> {
            self.actions.clone()
        }
        fn press(&self) -> bool {
            self.performed.borrow_mut().push("press".into());
            !self.fails
        }
        fn perform_named(&self, label: &str) -> bool {
            self.performed.borrow_mut().push(label.into());
            !self.fails
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeNode;
    use super::*;

    #[test]
    fn performs_named_action_of_matching_banner() {
        let banner = FakeNode::banner("Claude", &["Show details", "Allow once"]);
        let root = FakeNode::group(vec![banner.clone()]);
        assert_eq!(press(&root, &Rule::new("claude", "ALLOW ONCE")).outcome, Outcome::Pressed);
        assert_eq!(banner.performed(), vec!["Allow once"]);
    }

    #[test]
    fn presses_nested_button_when_app_is_in_parent() {
        let button = FakeNode::button("Merge");
        let toast = FakeNode::group(vec![FakeNode::text("contact-less"), FakeNode::group(vec![button.clone()])]);
        assert_eq!(press(&FakeNode::group(vec![toast]), &Rule::new("contact-less", "Merge")).outcome, Outcome::Pressed);
        assert_eq!(button.performed(), vec!["press"]);
    }

    #[test]
    fn picks_the_banner_of_the_right_app() {
        let slack = FakeNode::group(vec![FakeNode::text("Slack"), FakeNode::button("Allow once")]);
        let claude = FakeNode::group(vec![FakeNode::text("Claude"), FakeNode::button("Allow once")]);
        let root = FakeNode::group(vec![slack.clone(), claude.clone()]);
        assert_eq!(press(&root, &Rule::new("Claude", "Allow once")).outcome, Outcome::Pressed);
        assert!(slack.children[1].performed().is_empty());
        assert_eq!(claude.children[1].performed(), vec!["press"]);
    }

    #[test]
    fn leaves_other_apps_and_actions_alone() {
        let banner = FakeNode::banner("Slack", &["Allow once"]);
        assert_eq!(press(&banner, &Rule::new("Claude", "Allow once")).outcome, Outcome::NotFound);
        let banner = FakeNode::banner("Claude", &["Allow once"]);
        assert_eq!(press(&banner, &Rule::new("Claude", "Merge")).outcome, Outcome::NotFound);
        assert!(banner.performed().is_empty());
    }

    #[test]
    fn label_must_match_exactly() {
        let banner = FakeNode::banner("Claude", &["Allow once"]);
        assert_eq!(press(&banner, &Rule::new("Claude", "Allow")).outcome, Outcome::NotFound);
    }

    #[test]
    fn invalid_rule_matches_nothing() {
        let banner = FakeNode::banner("Claude", &["Allow once"]);
        assert_eq!(press(&banner, &Rule::new("", "Allow once")).outcome, Outcome::NotFound);
        assert_eq!(press(&banner, &Rule::new("Claude", "")).outcome, Outcome::NotFound);
        assert!(banner.performed().is_empty());
    }

    #[test]
    fn keeps_the_text_of_the_notification() {
        let toast = FakeNode::group(vec![
            FakeNode::text("Claude"),
            FakeNode::group(vec![FakeNode::text("Run ls -la ~/dev"), FakeNode::text("Claude")]),
            FakeNode::button("Allow once"),
        ]);
        let hit = press(&FakeNode::group(vec![toast]), &Rule::new("Claude", "Allow once"));
        assert_eq!(hit.texts, vec!["Claude", "Run ls -la ~/dev"]);
    }

    #[test]
    fn prefers_the_whole_description_to_cut_lines() {
        let long = "Claude, Bash: find ~/dev -name '*.rs' -newer Cargo.toml -exec wc -l {} +";
        let mut banner = FakeNode::banner("Claude", &["Allow once"]);
        banner.labels = vec![long.into()];
        banner.children = vec![FakeNode::text("Claude"), FakeNode::text("Bash: find ~/dev -name '*.rs' -newer…")];
        assert_eq!(press(&banner, &Rule::new("Claude", "Allow once")).texts, vec![long]);
    }

    #[test]
    fn keeps_lines_the_description_does_not_have() {
        let banner = FakeNode::banner("Claude", &["Allow once"]);
        assert_eq!(press(&banner, &Rule::new("Claude", "Allow once")).texts, vec!["Claude, run it?"]);
        let mut banner = FakeNode::banner("Claude", &["Allow once"]);
        banner.children.push(FakeNode::text("Run echo hi"));
        assert_eq!(press(&banner, &Rule::new("Claude", "Allow once")).texts, vec!["Claude, run it?", "Run echo hi"]);
    }

    #[test]
    fn find_does_not_press() {
        let banner = FakeNode::banner("Claude", &["Allow once"]);
        let mut second = FakeNode::banner("Claude", &["Allow once"]);
        second.labels = vec!["Claude, run that?".into()];
        let slack = FakeNode::banner("Slack", &["Allow once"]);
        let root = FakeNode::group(vec![banner.clone(), slack, second]);
        let hits = find_all(&root, &Rule::new("Claude", "Allow once"));
        assert_eq!(hits.iter().map(|h| h.texts.clone()).collect::<Vec<_>>(), vec![vec!["Claude, run it?"], vec!["Claude, run that?"]]);
        assert!(hits.iter().all(|h| h.outcome == Outcome::Found));
        assert!(banner.performed().is_empty());
        let button = FakeNode::button("Merge");
        let toast = FakeNode::group(vec![FakeNode::text("contact-less"), FakeNode::group(vec![button.clone()])]);
        assert_eq!(find_all(&FakeNode::group(vec![toast]), &Rule::new("contact-less", "Merge")).len(), 1);
        assert!(button.performed().is_empty());
        assert!(find_all(&root, &Rule::new("Claude", "Deny")).is_empty());
    }

    #[test]
    fn reports_failure() {
        let mut banner = FakeNode::banner("Claude", &["Allow once"]);
        banner.fails = true;
        assert_eq!(press(&banner, &Rule::new("Claude", "Allow once")).outcome, Outcome::Failed);
    }
}
