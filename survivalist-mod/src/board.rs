//! The work board: open work offers surfaced in the game's OWN
//! quest journal (docs/status.md "More to do (ecosystem-generated
//! work)"). Each work kind ships quest data in
//! story/Scripts/WorkBoard.xml; posting an offer spawns a real
//! QuestInstance (new-quest popup, journal line, map marker on
//! the target), claiming completes it, lapsing fails it. The XML
//! loads at STORY LOAD, so a hot reload alone runs blind: spawn
//! degrades to a log line and the offer stands without a journal
//! entry.

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use unityforge::mono::{self, LogLevel, MonoType};

use crate::common::{ctype, for_each_community, handle_of, own, with};

/// Every board quest's UniqueID starts with this; the orphan
/// sweep recognizes its own by it.
const BOARD_PREFIX: &str = "WorkBoard_";

/// Journal-entry handles owned by live offers (any work kind);
/// the orphan sweep spares them.
static OWNED: Mutex<Vec<i32>> = Mutex::new(Vec::new());

/// Spawn the journal entry for an offer via the game's own
/// QuestInstance.Spawn: new-quest notification, journal line, and
/// a map marker tracking `ob_h` (the mark, the raiders, ...).
/// Giver is the hirer's leader, seeker the player's leader; the
/// quest text's community names resolve from them. Returns the
/// instance handle; None (with a log line) when the quest data is
/// not loaded, since the XML loads at story load and a hot reload
/// alone cannot see it.
/// Stays here because it applies Survivalist's work-board quests rules through the game's classes, fields, content, and actions.
pub fn spawn(quest_id: &str, hirer_h: i32, ob_h: i32) -> Option<i32> {
    let quest_h = match find_quest(quest_id) {
        Ok(Some(h)) => h,
        Ok(None) => {
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: board: {quest_id} quest data not loaded; no journal entry (restart the story to load Scripts/WorkBoard.xml)"
                ),
            );
            return None;
        }
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: board: quest lookup failed: {e}"),
            );
            return None;
        }
    };
    let giver_h = with(hirer_h, |c| c.read_field("Leader").ok().as_ref().and_then(handle_of));
    let mut seeker_h: Option<i32> = None;
    let _ = for_each_community(|com| {
        if ctype(&com) == "Player" {
            seeker_h = com.read_field("Leader").ok().as_ref().and_then(handle_of);
            return Ok(false);
        }
        Ok(true)
    });
    let giver = giver_h.map(|h| json!({"handle": h})).unwrap_or(Json::Null);
    let seeker = seeker_h.map(|h| json!({"handle": h})).unwrap_or(Json::Null);
    let spawned = mono::invoke_static(
        "QuestInstance",
        "Spawn",
        &json!([{ "handle": quest_h }, giver, seeker, { "handle": ob_h }, false]),
    );
    drop(own(quest_h));
    if let Some(h) = giver_h {
        drop(own(h));
    }
    if let Some(h) = seeker_h {
        drop(own(h));
    }
    match spawned {
        Ok(v) => {
            let h = handle_of(&v);
            if let Some(h) = h {
                OWNED.lock().push(h);
            }
            h
        }
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: board: entry spawn failed: {e}"),
            );
            None
        }
    }
}

/// Walk GameImpl.Instance.CurrentStories and ask each loaded
/// story for the quest data (Story.FindQuestByUniqueID); the one
/// that loaded our XML answers.
/// Stays here because it applies Survivalist's work-board quests rules through the game's classes, fields, content, and actions.
fn find_quest(quest_id: &str) -> Result<Option<i32>, String> {
    let game = MonoType::find("GameImpl")
        .and_then(|t| t.singleton_instance())
        .ok_or("GameImpl.Instance not found")?;
    let Some(list_h) = handle_of(&game.read_field("CurrentStories")?) else {
        return Ok(None);
    };
    let list = own(list_h);
    let n = list.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    for i in 0..n {
        let Some(story_h) = handle_of(&list.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        let story = own(story_h);
        if let Ok(q) = story.invoke("FindQuestByUniqueID", &json!([quest_id])) {
            if let Some(qh) = handle_of(&q) {
                return Ok(Some(qh));
            }
        }
    }
    Ok(None)
}

/// Resolve a journal entry: Complete (claimed) or Fail (lapsed or
/// void), both the game's own paths with their own notifications.
/// Consumes the handle.
/// Stays here because it applies Survivalist's work-board quests rules through the game's classes, fields, content, and actions.
pub fn close(quest_h: Option<i32>, claimed: bool) {
    let Some(h) = quest_h else { return };
    OWNED.lock().retain(|&x| x != h);
    // The 1-arg overloads (skipCompletionEvents: false) avoid any
    // 0-arg/1-arg overload ambiguity in the shim's resolution.
    let method = if claimed { "Complete" } else { "Fail" };
    if let Err(e) = with(h, |q| q.invoke(method, &json!([false]))) {
        mono::log(
            LogLevel::Warn,
            &format!("survivalist-mod: board: {method} failed: {e}"),
        );
    }
    drop(own(h));
}

/// Delete every active board entry no live offer owns: a prior
/// generation's (hot reload) or a loaded save's entries would
/// linger in the journal forever. Entries owned by live offers of
/// ANY work kind are spared (matched by instance UniqueID, since
/// handles from separate bridge calls never match).
/// Stays here because it applies Survivalist's work-board quests rules through the game's classes, fields, content, and actions.
pub fn sweep_orphans() {
    let owned_handles: Vec<i32> = OWNED.lock().clone();
    let owned_uids: Vec<String> = owned_handles
        .iter()
        .filter_map(|&h| {
            with(h, |q| {
                q.invoke("GetUniqueID", &json!([]))
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
            })
        })
        .collect();
    let Some(sm) = MonoType::find("StoryManager").and_then(|t| t.singleton_instance()) else {
        return;
    };
    let Some(list_h) = sm.read_field("ActiveQuests").ok().as_ref().and_then(handle_of) else {
        return;
    };
    let list = own(list_h);
    let n = list
        .invoke("get_Count", &json!([]))
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    // Collect first: Delete mutates the list.
    let mut orphans = Vec::new();
    for i in 0..n {
        let Some(h) = list
            .invoke("get_Item", &json!([i]))
            .ok()
            .as_ref()
            .and_then(handle_of)
        else {
            continue;
        };
        let q = own(h);
        let uid = q
            .invoke("GetUniqueID", &json!([]))
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        if uid.starts_with(BOARD_PREFIX) && !owned_uids.contains(&uid) {
            std::mem::forget(q);
            orphans.push(h);
        }
    }
    let count = orphans.len();
    for h in orphans {
        let _ = with(h, |q| q.invoke("Delete", &json!([])));
        drop(own(h));
    }
    if count > 0 {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: board: swept {count} orphaned entries from a prior generation or save"
            ),
        );
    }
}
