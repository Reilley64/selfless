use crate::atoms::csrf_field::csrf_field;
use crate::components::button::button;
use crate::templates::document::document;
use topcoat::view::View;
use topcoat::view::attributes;
use topcoat::view::component;
use topcoat::view::view;

#[component]
pub(crate) async fn sign_in_template(failed: bool) -> topcoat::Result<impl View> {
    Ok(view! {
        document(
            <main class="flex min-h-svh items-center justify-center p-4">
                <form method="post" action="/admin/sign-in" class="flex w-full max-w-sm flex-col gap-4">
                    <h1 class="text-lg font-semibold">"selfless admin"</h1>
                    csrf_field()
                    if failed {
                        <p class="text-sm text-destructive">"Wrong name or password."</p>
                    }
                    <label class="flex flex-col gap-2 text-sm font-medium">
                        "Name"
                        <input
                            type="text"
                            name="name"
                            autocomplete="username"
                            required=(true)
                            class="h-9 rounded-lg border border-border bg-transparent px-3 font-normal outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        />
                    </label>
                    <label class="flex flex-col gap-2 text-sm font-medium">
                        "Password"
                        <input
                            type="password"
                            name="password"
                            autocomplete="current-password"
                            required=(true)
                            class="h-9 rounded-lg border border-border bg-transparent px-3 font-normal outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        />
                    </label>
                    button(attrs: attributes! { type="submit" }, "Sign in")
                </form>
            </main>
        )
    })
}
