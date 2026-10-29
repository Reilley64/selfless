use infrastructure::AppState;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::RouterBuilder;
use topcoat::router::page;
use topcoat::view::Unescaped;
use topcoat::view::View;
use topcoat::view::view;

pub fn register(router: RouterBuilder) -> RouterBuilder {
    router.page(home)
}

#[page("/admin")]
async fn home(cx: &Cx) -> topcoat::Result<impl View> {
    let build_info = &app_context::<AppState>(cx).build_info;
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <title>"selfless admin"</title>
                <style>(Unescaped::new_unchecked(include_str!("../dist/styles.css")))</style>
            </head>
            <body class="bg-background p-web-inset">
                <main class="flex flex-col gap-1">
                    <p class="font-sans font-bold text-text-primary">(build_info.name)</p>
                    <p class="font-sans text-text-secondary">(build_info.version)</p>
                </main>
            </body>
        </html>
    })
}
