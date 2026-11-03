use crate::ACCOUNT_PASSWORD_PATH;
use crate::ADMINS_PATH;
use crate::SIGN_OUT_PATH;
use crate::SYNC_PATH;
use crate::USERS_PATH;
use crate::atoms::csrf_field::csrf_field;
use crate::components::button::ButtonSize;
use crate::components::button::ButtonVariant;
use crate::components::button::button;
use crate::components::dropdown_menu::ITEM;
use crate::components::dropdown_menu::dropdown_menu;
use crate::components::dropdown_menu::dropdown_menu_content;
use crate::components::dropdown_menu::dropdown_menu_item;
use crate::components::dropdown_menu::dropdown_menu_trigger;
use crate::components::floating::FloatingAlign;
use crate::components::floating::FloatingSide;
use crate::components::icons::X;
use crate::components::sidebar::SidebarMenuButtonSize;
use crate::components::sidebar::SidebarMenuButtonVariant;
use crate::components::sidebar::sidebar;
use crate::components::sidebar::sidebar_content;
use crate::components::sidebar::sidebar_footer;
use crate::components::sidebar::sidebar_group;
use crate::components::sidebar::sidebar_header;
use crate::components::sidebar::sidebar_inset;
use crate::components::sidebar::sidebar_menu;
use crate::components::sidebar::sidebar_menu_button;
use crate::components::sidebar::sidebar_menu_button_variants;
use crate::components::sidebar::sidebar_menu_item;
use crate::components::sidebar::sidebar_provider;
use crate::components::sidebar::sidebar_trigger;
use crate::templates::document::document;
use infrastructure::AppState;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::icon::icon;
use topcoat::view::Child;
use topcoat::view::View;
use topcoat::view::attributes;
use topcoat::view::component;
use topcoat::view::view;

pub(crate) const PAGE_TITLE_ID: &str = "page-title";
const SIDEBAR_ID: &str = "sidebar";
const ACCOUNT_MENU_ID: &str = "account-menu";
const SECTIONS: [(&str, &str); 3] = [
    (SYNC_PATH, "Sync status"),
    (USERS_PATH, "Users"),
    (ADMINS_PATH, "Admins"),
];

#[component]
pub(crate) async fn admin_page(
    cx: &Cx,
    path: &str,
    title: &str,
    #[default] child: Child<'_>,
) -> topcoat::Result<impl View> {
    let build_info = &app_context::<AppState>(cx).build_info;
    Ok(view! {
        document(
            sidebar_provider(
                sidebar(
                    sheet_attrs: attributes! { id=(SIDEBAR_ID) },
                    sidebar_header(
                        <div class="flex items-center justify-between">
                            <span class="px-2 text-sm font-semibold">"selfless admin"</span>
                            button(
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Icon,
                                attrs: attributes! {
                                    type="button"
                                    class="md:hidden"
                                    commandfor=(SIDEBAR_ID)
                                    command="close"
                                    aria-label="Close sidebar"
                                },
                                icon(data: X)
                            )
                        </div>
                    )
                    sidebar_content(
                        sidebar_group(
                            sidebar_menu(
                                for (section_path, label) in SECTIONS {
                                    sidebar_menu_item(
                                        sidebar_menu_button(
                                            href: Some(section_path),
                                            active: section_path == path,
                                            <span>(label)</span>
                                        )
                                    )
                                }
                            )
                        )
                    )
                    sidebar_footer(
                        dropdown_menu(
                            dropdown_menu_trigger(
                                target: ACCOUNT_MENU_ID,
                                attrs: attributes! {
                                    class=(sidebar_menu_button_variants(
                                        SidebarMenuButtonVariant::Default,
                                        SidebarMenuButtonSize::Md,
                                    ))
                                },
                                <span>"Account"</span>
                            )
                            dropdown_menu_content(
                                id: ACCOUNT_MENU_ID,
                                side: FloatingSide::Top,
                                align: FloatingAlign::Start,
                                <a href=(ACCOUNT_PASSWORD_PATH) class=(ITEM)>"Change my password"</a>
                                <form method="post" action=(SIGN_OUT_PATH)>
                                    csrf_field()
                                    dropdown_menu_item(attrs: attributes! { type="submit" }, "Sign out")
                                </form>
                            )
                        )
                        <div class="px-2 text-xs text-muted-foreground">
                            <p>(build_info.name)</p>
                            <p>(build_info.version)</p>
                        </div>
                    )
                )
                sidebar_inset(
                    sidebar_header(
                        sidebar_trigger(
                            attrs: attributes! {
                                class="md:hidden"
                                commandfor=(SIDEBAR_ID)
                                command="show-modal"
                            }
                        )
                        <h1 id=(PAGE_TITLE_ID) class="text-base font-semibold">(title)</h1>
                    )
                    <div class="flex flex-col gap-6 p-6">(child)</div>
                )
            )
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::test::ADMIN_NAME;
    use crate::test::ADMIN_PASSWORD;
    use crate::test::create_admin;
    use crate::test::serve;
    use crate::test::signed_in;
    use test_app::TestApp;

    #[tokio::test]
    async fn every_admin_page_shows_the_server_name_and_version_and_links_every_section() {
        let app = TestApp::new().await;
        create_admin(&app, ADMIN_NAME, ADMIN_PASSWORD).await;
        let server = serve(&app).await;
        let mut browser = signed_in(&server, ADMIN_NAME, ADMIN_PASSWORD).await;

        for path in [
            "/admin/sync",
            "/admin/users",
            "/admin/admins",
            "/admin/account/password",
        ] {
            let page = browser.get(path).await;

            let build_info = &app.state.build_info;
            for shown in [build_info.name, build_info.version] {
                assert!(page.body.contains(&format!(">{shown}<")), "{}", page.body);
            }
            assert_eq!(page.link("Sync status"), "/admin/sync");
            assert_eq!(page.link("Users"), "/admin/users");
            assert_eq!(page.link("Admins"), "/admin/admins");
            assert_eq!(page.link("Change my password"), "/admin/account/password");
            assert_eq!(page.form_action("Sign out"), "/admin/sign-out");
        }
    }
}
