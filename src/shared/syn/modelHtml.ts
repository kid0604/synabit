/**
 * What DOMPurify must never keep in HTML the model wrote.
 *
 * `style` and `<style>`: an answer could restyle the screen around it — a
 * `position: fixed` block laid over the permission card's buttons.
 *
 * Forms and their controls: DOMPurify keeps `<form action>`, `<input>` and
 * `<button>` by default, and a form needs no script to send what it holds. An
 * answer steered by a page Syn read could end in a "Show details" button whose
 * form carries vault data to somebody else's server in the query string. The
 * CSP's `form-action 'none'` is the second lock; this is the first.
 *
 * Buttons the app adds itself (citations) are built after sanitising, from
 * nothing the model wrote, so forbidding `<button>` here does not touch them.
 */
export const MODEL_FORBID_TAGS = [
  'style',
  'form',
  'input',
  'button',
  'textarea',
  'select',
  'option',
  'fieldset',
];

export const MODEL_FORBID_ATTR = ['style', 'action', 'formaction'];
