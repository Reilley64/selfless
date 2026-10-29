use crate::atoms::csrf_field::csrf_field;
use crate::templates::document::document;
use topcoat::view::View;
use topcoat::view::component;
use topcoat::view::view;

#[component]
pub(crate) async fn sign_in_template(failed: bool) -> topcoat::Result<impl View> {
    Ok(view! {
        document(
            <main class="flex flex-col gap-1">
                <form method="post" action="/admin/sign-in" class="flex flex-col gap-1">
                    csrf_field()
                    if failed {
                        <p class="font-sans text-text-primary">"Wrong name or password."</p>
                    }
                    <label class="font-sans text-text-primary">
                        "Name"
                        <input type="text" name="name" autocomplete="username" required=(true) />
                    </label>
                    <label class="font-sans text-text-primary">
                        "Password"
                        <input type="password" name="password" autocomplete="current-password" required=(true) />
                    </label>
                    <button type="submit" class="font-sans text-text-primary">"Sign in"</button>
                </form>
            </main>
        )
    })
}
