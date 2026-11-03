use topcoat::view::Child;
use topcoat::view::Unescaped;
use topcoat::view::View;
use topcoat::view::component;
use topcoat::view::view;

#[component]
pub(crate) async fn document(#[default] child: Child<'_>) -> topcoat::Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <title>"selfless admin"</title>
                <style>(Unescaped::new_unchecked(include_str!("../../dist/styles.css")))</style>
            </head>
            <body class="bg-background p-web-inset">
                (child)
            </body>
        </html>
    })
}
