// Add-profile-modal component — encapsulated as a function returning a
// real DOM node, mounted onto the shared `#root` container (see
// `../../root.html`). Keeps its own `#modal` id so `add_profile.css`'s
// selectors need no change from before this component-folder split.
// Field values are read directly off the DOM by Rust
// (`ChromeEngine::input_value`), so this component only wires the two
// buttons. `create` reuses the shared `.btn.active` look (same blue
// `#create` always had) via the `active` prop instead of a bundle-local
// class.
function AddProfile() {
  const field = (id, label) => Row({}, Label({}, label), Input({ id, type: "text" }));

  return Div(
    { id: "modal" },
    field("field-name", "Name"),
    field("field-start-url", "Start URL"),
    field("field-email", "Email"),
    field("field-password", "Password"),
    field("field-proxy", "Proxy"),
    Div({ id: "error", className: "error" }),
    Div(
      { id: "buttons" },
      Button({ id: "create", active: true, onClick: atomic.createProfileSubmit }, "Create"),
      Button({ id: "cancel", onClick: atomic.cancelAddProfile }, "Cancel"),
    ),
  );
}

mount("root", AddProfile());
