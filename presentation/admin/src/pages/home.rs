use crate::atoms::csrf_field::csrf_field;
use crate::require_admin;
use crate::templates::document::document;
use infrastructure::AppState;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::page;
use topcoat::view::View;
use topcoat::view::view;

#[page("/admin")]
pub(crate) async fn home(cx: &Cx) -> topcoat::Result<impl View> {
    require_admin(cx).await?;
    let build_info = &app_context::<AppState>(cx).build_info;
    Ok(view! {
        document(
            <main class="flex flex-col gap-1">
                <p class="font-sans font-bold text-text-primary">(build_info.name)</p>
                <p class="font-sans text-text-secondary">(build_info.version)</p>
                <form method="post" action="/admin/sign-out">
                    csrf_field()
                    <button type="submit" class="font-sans text-text-primary">"Sign out"</button>
                </form>
            </main>
        )
    })
}
