import { VueRenderer } from '@tiptap/vue-3';
import tippy, { type Instance as TippyInstance } from 'tippy.js';
import type { Component } from 'vue';

/**
 * The `render` of a Tiptap suggestion — the slash menu, `@` mentions, `:`
 * emoji — as a tippy popup holding `menu`.
 *
 * Since @tiptap/suggestion 3.31, `onStart` arrives with no items: they are
 * fetched afterwards and handed over in an `onUpdate`. So the menu's root must
 * be rendered even while empty (`v-show`, not `v-if`), or the popup is given
 * `null` as its content and never shows; and the popup shows and hides with
 * the items rather than opening on start. Enter still picked a command
 * through the hidden menu, which is how the slash menu looked fine to anyone
 * who knew the names.
 *
 * `showEmpty` is for a menu that has something to say when nothing matched
 * (the `@` menu's "no match" hint). It shows once the search has answered,
 * not while it is still running, so the hint does not flash on every key.
 */
export function suggestionPopup(menu: Component, { showEmpty = false } = {}) {
  return () => {
    let component: VueRenderer | undefined;
    let popup: TippyInstance | undefined;

    const place = (props: any) => {
      if (!popup || !props.clientRect) return;
      popup.setProps({ getReferenceClientRect: props.clientRect });
      if (props.items.length || (showEmpty && !props.loading)) popup.show();
      else popup.hide();
    };

    return {
      onStart: (props: any) => {
        component = new VueRenderer(menu, { props, editor: props.editor });
        popup = tippy(document.body, {
          appendTo: () => document.body,
          content: component.element as Element,
          showOnCreate: false,
          interactive: true,
          trigger: 'manual',
          placement: 'bottom-start',
        });
        place(props);
      },
      onUpdate: (props: any) => {
        // Each key brings an update with no items while the new ones load,
        // then one with them. The menu keeps what it shows until they arrive
        // — the same array, so the selection is not reset either — and takes
        // the rest (the range, `command`) now.
        if (props.loading && component) {
          props = { ...props, items: component.props.items ?? [] };
        }
        component?.updateProps(props);
        place(props);
      },
      onKeyDown: (props: any) => {
        // The plugin ends the suggestion itself on Escape.
        if (props.event.key === 'Escape') {
          popup?.hide();
          return true;
        }
        return component?.ref?.onKeyDown(props.event) ?? false;
      },
      onExit: () => {
        popup?.destroy();
        component?.destroy();
        popup = undefined;
        component = undefined;
      },
    };
  };
}
