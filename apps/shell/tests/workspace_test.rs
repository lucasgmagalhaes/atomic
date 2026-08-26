use shell::workspace::WorkspaceManager;

#[test]
fn starts_with_one_default_active_workspace() {
    let wm = WorkspaceManager::new();
    assert_eq!(wm.workspaces().len(), 1);
    assert_eq!(wm.active().name, "Principal");
    assert_eq!(wm.active_index(), 0);
}

#[test]
fn create_adds_a_new_workspace_without_changing_active() {
    let mut wm = WorkspaceManager::new();
    let idx = wm.create("Farm squad");
    assert_eq!(idx, 1);
    assert_eq!(wm.workspaces().len(), 2);
    assert_eq!(wm.active_index(), 0);
}

#[test]
fn set_active_switches_and_rejects_out_of_range() {
    let mut wm = WorkspaceManager::new();
    wm.create("Trades");
    assert!(wm.set_active(1));
    assert_eq!(wm.active().name, "Trades");
    assert!(!wm.set_active(5));
    assert_eq!(wm.active_index(), 1);
}

#[test]
fn add_and_remove_profile() {
    let mut wm = WorkspaceManager::new();
    assert!(wm.add_profile(0, "conta-1"));
    assert_eq!(wm.active().profiles(), ["conta-1"]);
    assert!(wm.remove_profile(0, "conta-1"));
    assert!(wm.active().profiles().is_empty());
}

#[test]
fn add_profile_is_idempotent() {
    let mut wm = WorkspaceManager::new();
    wm.add_profile(0, "conta-1");
    wm.add_profile(0, "conta-1");
    assert_eq!(wm.active().profiles().len(), 1);
}

#[test]
fn find_profile_locates_its_workspace() {
    let mut wm = WorkspaceManager::new();
    let farm = wm.create("Farm squad");
    wm.add_profile(farm, "conta-7");
    assert_eq!(wm.find_profile("conta-7"), Some(farm));
    assert_eq!(wm.find_profile("nope"), None);
}

#[test]
fn move_profile_relocates_between_workspaces() {
    let mut wm = WorkspaceManager::new();
    let farm = wm.create("Farm squad");
    wm.add_profile(0, "conta-1");

    assert!(wm.move_profile("conta-1", farm));

    assert!(wm.workspaces()[0].profiles().is_empty());
    assert_eq!(wm.workspaces()[farm].profiles(), ["conta-1"]);
}

#[test]
fn remove_refuses_to_drop_the_last_workspace() {
    let mut wm = WorkspaceManager::new();
    assert!(!wm.remove(0));
    assert_eq!(wm.workspaces().len(), 1);
}

#[test]
fn remove_clamps_active_index_when_needed() {
    let mut wm = WorkspaceManager::new();
    wm.create("B");
    wm.create("C");
    wm.set_active(2); // "C"

    assert!(wm.remove(2));

    assert_eq!(wm.workspaces().len(), 2);
    assert_eq!(wm.active_index(), 1); // clamped back into range
}
