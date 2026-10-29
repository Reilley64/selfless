use topcoat::{
    Result,
    icon::icon,
    view::{Attributes, Child, StaticClass, View, attributes, class, component, view},
};

use super::icons::CHEVRON_DOWN;

/// Classes for the select control, with space for a custom dropdown arrow.
const SELECT: StaticClass = class!(
    "h-9 w-full appearance-none items-center rounded-lg border border-border",
    "bg-transparent pr-8 pl-3 text-left text-sm transition-colors outline-none",
    "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
    "aria-invalid:border-destructive aria-invalid:focus-visible:ring-destructive",
    "focus-visible:ring-offset-background disabled:pointer-events-none",
);

/// Classes for browsers that support customizable select pickers. Other browsers use
/// their native picker.
const PICKER: StaticClass = class!(
    "[&::picker(select)]:[appearance:base-select]",
    "[&::picker(select)]:mt-1 [&::picker(select)]:rounded-lg",
    "[&::picker(select)]:border [&::picker(select)]:border-border",
    "[&::picker(select)]:bg-popover [&::picker(select)]:p-1",
    "[&::picker(select)]:text-popover-foreground [&::picker(select)]:shadow-sm",
    "[&::picker-icon]:hidden",
    "[&_optgroup>legend]:px-2 [&_optgroup>legend]:py-1.5",
    "[&_optgroup>legend]:text-xs [&_optgroup>legend]:font-medium",
    "[&_optgroup>legend]:text-muted-foreground [&_optgroup>legend]:cursor-default",
    "[&_optgroup>legend]:select-none",
    "[&_option]:flex [&_option]:items-center [&_option]:gap-2 [&_option]:rounded-md",
    "[&_option]:px-2 [&_option]:py-1.5 [&_option]:text-sm [&_option]:outline-none",
    "[&_option:hover]:bg-foreground/5 [&_option:focus]:bg-foreground/5",
    "[&_option:checked]:font-medium",
    "[&_option::checkmark]:order-1 [&_option::checkmark]:ml-auto",
    "[&_option::checkmark]:size-4 [&_option::checkmark]:shrink-0",
    "[&_option::checkmark]:content-[''] [&_option::checkmark]:bg-muted-foreground",
    "[&_option::checkmark]:[mask-size:100%_100%]",
    "[&_option::checkmark]:[mask-image:var(--select-checkmark)]",
);

/// A styled native select control.
///
/// Pass `<option>` or `<optgroup>` elements as children. Classes in `attrs` apply to
/// the wrapper, while other attributes and event handlers go on the `<select>`. The
/// control fills its container by default. Set `aria-invalid="true"` to show the error
/// border and focus ring.
///
/// Browsers with customizable select support also style the picker. For a styled group
/// heading, place a `<legend>` first inside an `<optgroup>` and keep its `label`
/// attribute for browsers that use the native picker.
///
/// ```ignore
/// view! {
///     select(
///         attrs: attributes! { name="region" },
///         <option>"eu-central-1"</option>
///         <option>"us-east-1"</option>
///     )
/// }
/// ```
#[component]
pub async fn select(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    // `appearance: base-select` opts into the customizable picker. It is set
    // from the wrapper because the descendant selector outranks the
    // `appearance-none` fallback in specificity, making the outcome
    // independent of stylesheet order; browsers without support drop the
    // invalid declaration and keep the fallback.
    Ok(view! {
        <span
            class=(class!(
                "relative block has-[:disabled]:opacity-50",
                "[&>select]:[appearance:base-select]",
                "[&:has(select:open)>svg]:rotate-180",
                attrs.remove("class"),
            ))
        >
            <select class=(class!(SELECT, PICKER)) (attrs)>(child)</select>
            icon(
                data: CHEVRON_DOWN,
                attrs: attributes! {
                    class="pointer-events-none absolute top-1/2 right-3 size-4 \
                        -translate-y-1/2 text-muted-foreground transition-transform"
                }
            )
        </span>
    })
}
