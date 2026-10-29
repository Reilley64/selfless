# Media server feature split across Jellyfin, Plex and Emby

Research for [SELFLESS-5](https://app.plane.so/reilley/browse/SELFLESS-5/). The sources were read on 2026-10-29, when the current releases were Jellyfin 12.4, Plex Media Server 1.43.5.11079 and Emby Server 4.10.2.0.

## Summary

All three servers give every user the same core for movies and TV shows at no cost. They scan movie and show folders, attach metadata and artwork, and read Kodi-style NFO files. They support several users with per-library access and keep watched and resume state for each user. They show continue watching and next-up rows, and they offer collections and playlists. They direct play when the client can and transcode in software when it cannot. They handle embedded and external subtitles and audio tracks, group multiple versions of one movie, show local extras and trailers, and show preview thumbnails while scrubbing.

The extras fall into two groups. The first group is features that Plex and Emby sell and Jellyfin gives away: hardware transcoding, HDR to SDR tone mapping, intro skipping, offline downloads, webhooks, cinema trailers before a movie, and, for Emby, live TV. The second group is features that only one or two of them have: credit skipping, watch-together sessions, editions for different cuts, pre-transcoded "optimized" copies, LDAP sign-in and Plex's online extras.

For selfless, the free core shared by all three is the strongest MVP candidate. Hardware transcoding on Quick Sync and intro skipping are the extras with the best case for early inclusion. Live TV, downloads, watch-together and cinema trailers look like later work or out of scope. The open questions at the end list what a later grilling ticket must settle.

## Versions checked

| Product | Version | Released | Source |
| --- | --- | --- | --- |
| Jellyfin | 12.4 (stable) | 2026-10-26 | [GitHub releases][jf-releases] |
| Plex Media Server | 1.43.5.11079 (public) | 2026-10-26 | [Plex forum release thread][plex-releases] |
| Emby Server | 4.10.2.0 (stable), 4.11.0.12 (beta) | 2026-10-20, 2026-10-28 | [GitHub releases][emby-releases] |

Jellyfin dropped the leading "10." from its version numbers with 12.0, so 12.x follows 10.11.x ([Jellyfin 12.0 post][jf-12]).

## How each product draws the line

### Jellyfin

Jellyfin has no paid tier. Its about page says there are "no premium licenses or features" ([about][jf-about]). The split that matters is between the server core, official plugins and third-party plugins. Several features that Plex and Emby build in or sell come to Jellyfin as plugins: online subtitle search, webhooks, LDAP, pre-roll intros and the detection of intros and credits ([plugins][jf-plugins], [media segments][jf-segments]). Jellyfin is GPL, so selfless can read it for ideas only.

### Plex

Plex has three levels: free, Remote Watch Pass and Plex Pass ([plans][plex-plans]). Free covers local streaming, collections, metadata editing, Plex Home managed accounts, playlists, library sharing and the Media Optimizer. Remote Watch Pass adds remote streaming and nothing else. Plex Pass adds everything else on the list ([plans][plex-plans], [Plex Pass overview][plex-pass]). Since April 2025, remote streaming of personal media needs either a Plex Pass on the server admin account or a Remote Watch Pass on the viewer's account ([plans FAQ][plex-plans], [remote access][plex-remote]). Remote access needs the server signed in to a plex.tv account, and some apps, such as PlayStation, Xbox and smart TVs, work only when both app and server are signed in ([remote access][plex-remote], [link an app][plex-link]).

### Emby

Emby has free and Emby Premiere. The admin installs the Premiere key on the server, and the server's status decides what every connected client gets ([Premiere][emby-premiere], [feature matrix][emby-matrix]). Free covers browsing, library organization and metadata, users, parental controls and Emby Connect. Premiere adds intro skipping, cinema intros, live TV and DVR, hardware transcoding, tone mapping, in-playback subtitle search, downloads and sync, LDAP, content conversion and webhooks ([feature matrix][emby-matrix]). Some apps also limit free playback. Desktop, game console and mobile apps play only one minute without Premiere or an app unlock, and Android TV plays free on up to 5 TV devices. Apple TV, Roku and LG and Samsung smart TVs play in full for free ([feature matrix][emby-matrix]).

## Feature matrix for movies and TV shows

"Free" means built in at no cost. "Plugin" means an official Jellyfin plugin unless the row says otherwise. "Pass" means Plex Pass. "Premiere" means Emby Premiere.

| Feature | Jellyfin | Plex | Emby |
| --- | --- | --- | --- |
| Movie and show libraries from folder naming | Free ([movies][jf-movies], [shows][jf-shows]) | Free ([multi-version][plex-multiversion]) | Free ([movie naming][emby-movie-naming]) |
| Online metadata and artwork | Free ([movies][jf-movies]) | Free ([plans][plex-plans]) | Free ([feature matrix][emby-matrix]) |
| Read local NFO files | Free, always on, beats online providers ([NFO][jf-nfo]) | Free, Plex NFO agent, PMS 1.43.1 or newer ([NFO][plex-nfo]) | Free, Nfo Metadata reader ([4.8.3.0 notes][emby-4830]) |
| Several users with per-library access | Free ([users][jf-users]) | Free, Plex Home up to 15 users ([Plex Home][plex-home]) | Free ([users][emby-users], [feature matrix][emby-matrix]) |
| Easier sign-in on a TV | Quick Connect 6-character code ([Quick Connect][jf-quick-connect]) | 4-character code at plex.tv/link ([link an app][plex-link]) | Emby Connect account, optional ([Emby Connect][emby-connect]) |
| Parental controls by rating and tag | Free, plus access schedules ([users][jf-users]) | Preset profiles free, custom rating and label rules need Pass ([restrictions][plex-restrictions], [managed accounts][plex-managed]) | Free, plus access schedules ([parental controls][emby-parental]) |
| Watched state, resume, continue watching, next up | Free ([12.0 post][jf-12]) | Free, "On Deck" ([webhooks][plex-webhooks]) | Free ([home screen][emby-home]) |
| Collections | Free, TMDb Box Sets plugin for auto sets ([plugins][jf-plugins]) | Free ([plans][plex-plans]) | Free, auto sets from library options ([collections][emby-collections]) |
| Playlists | Free ([12.0 post][jf-12]) | Free ([plans][plex-plans]) | Free ([downloads][emby-sync]) |
| Direct play, remux and software transcoding | Free ([transcoding][jf-transcoding]) | Free ([hardware streaming][plex-hw]) | Free ([playback methods][emby-playback]) |
| Hardware transcoding | Free, Intel, AMD, Nvidia, Apple, Rockchip ([transcoding][jf-transcoding]) | Pass ([hardware streaming][plex-hw]) | Premiere, except Nvidia Shield and WD NAS ([feature matrix][emby-matrix]) |
| HDR to SDR tone mapping | Free ([transcoding][jf-transcoding]) | Pass ([tone mapping][plex-tonemap]) | Premiere ([feature matrix][emby-matrix]) |
| Embedded and external subtitles and audio tracks | Free ([movies][jf-movies]) | Free ([local subtitles][plex-local-subs]) | Free ([subtitles][emby-subtitles]) |
| Online subtitle search | Open Subtitles plugin ([plugins][jf-plugins]) | Free, OpenSubtitles.com ([subtitle search][plex-subs]) | Automatic download via plugin, in-playback search needs Premiere ([auto subtitles][emby-auto-subs], [feature matrix][emby-matrix]) |
| Multiple versions of one item | Free for movies, episodes since 12.0 ([12.0 post][jf-12]) | Free ([multi-version][plex-multiversion]) | Free, up to 8 versions ([movie naming][emby-movie-naming]) |
| Editions (theatrical, director's cut) as separate items | Not documented, versions only | Pass ([editions][plex-editions]) | Versions only, "may be some limitations" for cuts ([movie naming][emby-movie-naming]) |
| Local extras and trailers | Free ([movies][jf-movies]) | Free ([local extras][plex-extras]) | Free ([trailers][emby-trailers], [movie naming][emby-movie-naming]) |
| Online extras and trailers | Not built in | Pass, streamed from a licensed provider ([cinema trailers and extras][plex-cinema]) | Trailer plugin plus Premiere ([cinema intros][emby-cinema]) |
| Preview thumbnails while scrubbing | Free, "trickplay" since 10.9 ([10.9 post][jf-109]) | Free ([preview thumbnails][plex-preview]) | Free, not on the Premiere list ([4.7.0.60 notes][emby-4760], [feature matrix][emby-matrix]) |
| Skip intro | Media segments since 10.10, detection by plugin ([media segments][jf-segments], [10.10 post][jf-1010]) | Pass for server admin and viewer ([skip intros][plex-skip]) | Premiere ([intro skip][emby-intro]) |
| Skip credits | Outro segment type, detection by plugin ([media segments][jf-segments]) | Pass ([Plex Pass overview][plex-pass]) | Not documented |
| Trailers or pre-rolls before a movie | Local Intros plugin ([plugins][jf-plugins]) | Pass ([plans][plex-plans]) | Premiere ([feature matrix][emby-matrix], [cinema intros][emby-cinema]) |
| Live TV from a tuner | Free ([live TV][jf-livetv]) | Free to watch ([DVR FAQ][plex-dvr]) | Premiere ([live TV][emby-livetv]) |
| DVR recording | Free ([live TV][jf-livetv]) | Pass ([DVR FAQ][plex-dvr]) | Premiere ([live TV][emby-livetv]) |
| Offline downloads | Original file only, no transcoded sync ([users][jf-users]) | Pass ([Plex Pass overview][plex-pass]) | Premiere ([feature matrix][emby-matrix], [downloads][emby-sync]) |
| Remote streaming | Free, you set up the networking ([networking][jf-networking], [users][jf-users]) | Pass or Remote Watch Pass ([plans][plex-plans]) | Free ([feature matrix][emby-matrix], [Emby Connect][emby-connect]) |
| Watch together | SyncPlay, free since 10.6 ([10.6 post][jf-106]) | Free, but dropped from new apps since 2025-02-25, web app only ([Watch Together][plex-watch-together]) | Not built in, third-party plugins exist |
| Outgoing webhooks | Webhook plugin ([notifications][jf-notifications]) | Pass ([webhooks][plex-webhooks]) | Premiere ([webhooks][emby-webhooks]) |
| Bandwidth and stream limits | Per-user remote bitrate limit, free ([users][jf-users]) | Pass ([plans][plex-plans]) | Concurrent stream limit needs Premiere ([feature matrix][emby-matrix]) |
| Pre-transcoded copies | No, "currently not available" ([users][jf-users]) | Free, Media Optimizer ([optimizer][plex-optimizer]) | Premiere, content conversion ([feature matrix][emby-matrix]) |
| LDAP sign-in | LDAP plugin ([plugins][jf-plugins]) | No, plex.tv accounts only ([link an app][plex-link]) | Premiere ([feature matrix][emby-matrix]) |

## Features every one of them has

These are free in all three products, so users of any of them expect them:

1. Movie and TV libraries built from folder and file naming, with series, season and episode levels.
2. Metadata and artwork, from online providers or from local NFO files. Plex added NFO reading only in PMS 1.43.1, and its NFO agent does not support multi-part movies or NFO data for extras ([NFO][plex-nfo]).
3. Several users per server, each with access to chosen libraries.
4. Per-user watched state, resume points, a continue watching row and next-up episodes.
5. Collections and playlists.
6. Direct play when the client supports the file, and software transcoding when it does not.
7. Embedded and external subtitles with forced and hearing-impaired flags, and audio track choice.
8. Several versions of one movie grouped into one item, such as 1080p and 4K.
9. Local extras and trailers next to the movie or show.
10. Preview thumbnails on the scrub bar.

All three also make TV sign-in easier, but in different ways. Jellyfin uses a 6-character Quick Connect code, Plex uses a 4-character code at plex.tv/link, and Emby uses an optional Emby Connect account that saves typing the server address ([Quick Connect][jf-quick-connect], [link an app][plex-link], [Emby Connect][emby-connect]).

Parental controls by rating are close to universal. Jellyfin and Emby give them away, and Plex gives preset profiles for managed accounts but sells custom rules ([restrictions][plex-restrictions]).

## Extras

### Paid in Plex or Emby, free in Jellyfin

- Hardware transcoding and HDR to SDR tone mapping.
- Intro skipping. Jellyfin stores the segments but needs a plugin to detect them.
- Offline downloads. Jellyfin only downloads the original file.
- Webhooks for home automation and notifications.
- Trailers or custom pre-rolls before a movie.
- Live TV in Emby, and DVR recording in Plex and Emby.
- Remote streaming in Plex.

### Present in only one or two products

- Credit skipping is built into Plex Pass. Jellyfin has an outro segment type, and Emby documents intro skipping only.
- Watch-together sessions are in Jellyfin as SyncPlay. Plex keeps Watch Together in the web app only.
- Editions for different cuts, tracked separately, exist only in Plex Pass.
- Pre-transcoded copies exist in Plex for free and in Emby Premiere. Jellyfin does not have them.
- Online extras and trailers come from Plex Pass and from the Emby Trailer plugin.
- LDAP sign-in is in Jellyfin as a plugin and in Emby Premiere.
- Fast user switching with a PIN per profile is in Plex Home ([Plex Home][plex-home]).

## What this means for selfless

These are suggestions based on the evidence. The decision belongs to a later grilling ticket.

**MVP candidates.** The MVP list can start from the shared core above, limited to movies and TV. Radarr and Sonarr already produce NFO files, and all three products read Kodi-style NFO, so the NFO tag sets in the [Jellyfin NFO docs][jf-nfo] and the [Plex NFO docs][plex-nfo] are a good reference for what to parse. Per-user state, continue watching and next up are the features that make a household server usable, and all three treat them as free basics.

**Strong early extras.** The first is hardware transcoding on Quick Sync. The target host has an i5-12600H, and Plex and Emby both charge for this feature. The second is intro skipping, which is a headline paid feature in both Plex and Emby ([plans][plex-plans], [feature matrix][emby-matrix]). Jellyfin stores typed segments, such as intro, outro, recap, preview and commercial, and lets each client decide what to do with them. Plugins do the detection ([media segments][jf-segments]). Selfless could adopt the storage model first and add detection later.

**Likely later or out of scope.** These are live TV and DVR, offline downloads, watch-together, cinema trailers, online extras, pre-transcoded copies, LDAP and outgoing webhooks. None of them is in the shared core, and each one needs its own subsystem.

## Open questions

1. Is hardware transcoding in the MVP, or does the MVP rely on direct play and software transcoding for the Apple TV and Google TV apps?
2. Are parental controls by rating and tag in the MVP, given a household with several users?
3. How do users sign in on a TV app: a Quick Connect style code, a profile picker with PINs, or both?
4. Is intro and credit skipping in the MVP? If yes, where do the markers come from, since selfless takes metadata only from NFO files and webhooks and has no detection yet?
5. Are multiple versions in the MVP? This matters only if the library holds more than one file per movie or episode, for example separate 4K and 1080p copies.
6. Are local extras and trailers in the MVP? This depends on whether Radarr and Sonarr put extras in the library folders.
7. Is remote access outside the home network an MVP need, or is the MVP local only?
8. Should selfless send its own outgoing webhooks or events, or only receive them from Radarr and Sonarr?

## Sources

- jf-releases: <https://github.com/jellyfin/jellyfin/releases>
- jf-12: <https://jellyfin.org/posts/jellyfin-release-12.0/>
- jf-1010: <https://jellyfin.org/posts/jellyfin-release-10.10.0>
- jf-109: <https://jellyfin.org/posts/jellyfin-release-10.9.0>
- jf-106: <https://jellyfin.org/posts/jellyfin-10-6-0>
- jf-about: <https://jellyfin.org/docs/general/about>
- jf-plugins: <https://jellyfin.org/docs/general/server/plugins/>
- jf-segments: <https://jellyfin.org/docs/general/server/metadata/media-segments>
- jf-movies: <https://jellyfin.org/docs/general/server/media/movies>
- jf-shows: <https://jellyfin.org/docs/general/server/media/shows>
- jf-nfo: <https://jellyfin.org/docs/general/server/metadata/nfo>
- jf-users: <https://jellyfin.org/docs/general/server/users/adding-managing-users>
- jf-quick-connect: <https://jellyfin.org/docs/general/server/quick-connect>
- jf-transcoding: <https://jellyfin.org/docs/general/post-install/transcoding/>
- jf-livetv: <https://jellyfin.org/docs/general/server/live-tv/>
- jf-notifications: <https://jellyfin.org/docs/general/server/notifications>
- jf-networking: <https://jellyfin.org/docs/general/post-install/networking/>
- plex-releases: <https://forums.plex.tv/t/plex-media-server/30447>
- plex-plans: <https://www.plex.tv/plans/>
- plex-pass: <https://support.plex.tv/articles/201751006-plex-pass-feature-overview/>
- plex-remote: <https://support.plex.tv/articles/200289506-remote-access/>
- plex-link: <https://support.plex.tv/articles/203395277-connect-app-to-your-plex-account/>
- plex-nfo: <https://support.plex.tv/articles/using-nfo-metadata-files-with-plex/>
- plex-home: <https://support.plex.tv/articles/203815766-what-is-plex-home/>
- plex-managed: <https://support.plex.tv/articles/203948776-managed-users/>
- plex-restrictions: <https://support.plex.tv/articles/204232573-restricting-the-shares/>
- plex-webhooks: <https://support.plex.tv/articles/115002267687-webhooks/>
- plex-hw: <https://support.plex.tv/articles/115002178853-using-hardware-accelerated-streaming/>
- plex-tonemap: <https://support.plex.tv/articles/hdr-to-sdr-tone-mapping/>
- plex-local-subs: <https://support.plex.tv/articles/200471133-adding-local-subtitles-to-your-media/>
- plex-subs: <https://support.plex.tv/articles/subtitle-search/>
- plex-multiversion: <https://support.plex.tv/articles/200381043-multi-version-movies/>
- plex-editions: <https://support.plex.tv/articles/multiple-editions/>
- plex-extras: <https://support.plex.tv/articles/local-files-for-trailers-and-extras/>
- plex-cinema: <https://support.plex.tv/articles/202934883-cinema-trailers-extras/>
- plex-preview: <https://support.plex.tv/articles/202197528-video-preview-thumbnails/>
- plex-skip: <https://support.plex.tv/articles/skip-content/>
- plex-dvr: <https://support.plex.tv/articles/226463767-frequently-asked-questions-dvr-live-tv/>
- plex-watch-together: <https://support.plex.tv/articles/watch-together/>
- plex-optimizer: <https://support.plex.tv/articles/214079318-media-optimizer-overview/>
- emby-releases: <https://github.com/MediaBrowser/Emby.Releases/releases>
- emby-4830: <https://github.com/MediaBrowser/Emby.Releases/releases/tag/4.8.3.0>
- emby-4760: <https://github.com/MediaBrowser/Emby.Releases/releases/tag/4.7.0.60>
- emby-premiere: <https://emby.media/support/articles/Emby-Premiere.html>
- emby-matrix: <https://emby.media/support/articles/Premiere-Feature-Matrix.html>
- emby-users: <https://emby.media/support/articles/Users.html>
- emby-connect: <https://emby.media/support/articles/Emby-Connect.html>
- emby-parental: <https://emby.media/support/articles/Parental-Controls.html>
- emby-home: <https://emby.media/support/articles/apps/common/User-Prefs-HomeScreen.html>
- emby-collections: <https://emby.media/support/articles/Collections.html>
- emby-sync: <https://emby.media/support/articles/Sync.html>
- emby-playback: <https://emby.media/support/articles/DirectPlay-Stream-Transcoding.html>
- emby-subtitles: <https://emby.media/support/articles/Subtitles.html>
- emby-auto-subs: <https://emby.media/support/articles/Automatic-Subtitle-Downloads.html>
- emby-movie-naming: <https://emby.media/support/articles/Movie-Naming.html>
- emby-trailers: <https://emby.media/support/articles/Trailers.html>
- emby-cinema: <https://emby.media/support/articles/Cinema-Intros.html>
- emby-intro: <https://emby.media/support/articles/Intro-Skip.html>
- emby-livetv: <https://emby.media/support/articles/Live-TV.html>
- emby-webhooks: <https://emby.media/support/articles/Webhooks.html>

[jf-releases]: https://github.com/jellyfin/jellyfin/releases
[jf-12]: https://jellyfin.org/posts/jellyfin-release-12.0/
[jf-1010]: https://jellyfin.org/posts/jellyfin-release-10.10.0
[jf-109]: https://jellyfin.org/posts/jellyfin-release-10.9.0
[jf-106]: https://jellyfin.org/posts/jellyfin-10-6-0
[jf-about]: https://jellyfin.org/docs/general/about
[jf-plugins]: https://jellyfin.org/docs/general/server/plugins/
[jf-segments]: https://jellyfin.org/docs/general/server/metadata/media-segments
[jf-movies]: https://jellyfin.org/docs/general/server/media/movies
[jf-shows]: https://jellyfin.org/docs/general/server/media/shows
[jf-nfo]: https://jellyfin.org/docs/general/server/metadata/nfo
[jf-users]: https://jellyfin.org/docs/general/server/users/adding-managing-users
[jf-quick-connect]: https://jellyfin.org/docs/general/server/quick-connect
[jf-transcoding]: https://jellyfin.org/docs/general/post-install/transcoding/
[jf-livetv]: https://jellyfin.org/docs/general/server/live-tv/
[jf-notifications]: https://jellyfin.org/docs/general/server/notifications
[jf-networking]: https://jellyfin.org/docs/general/post-install/networking/
[plex-releases]: https://forums.plex.tv/t/plex-media-server/30447
[plex-plans]: https://www.plex.tv/plans/
[plex-pass]: https://support.plex.tv/articles/201751006-plex-pass-feature-overview/
[plex-remote]: https://support.plex.tv/articles/200289506-remote-access/
[plex-link]: https://support.plex.tv/articles/203395277-connect-app-to-your-plex-account/
[plex-nfo]: https://support.plex.tv/articles/using-nfo-metadata-files-with-plex/
[plex-home]: https://support.plex.tv/articles/203815766-what-is-plex-home/
[plex-managed]: https://support.plex.tv/articles/203948776-managed-users/
[plex-restrictions]: https://support.plex.tv/articles/204232573-restricting-the-shares/
[plex-webhooks]: https://support.plex.tv/articles/115002267687-webhooks/
[plex-hw]: https://support.plex.tv/articles/115002178853-using-hardware-accelerated-streaming/
[plex-tonemap]: https://support.plex.tv/articles/hdr-to-sdr-tone-mapping/
[plex-local-subs]: https://support.plex.tv/articles/200471133-adding-local-subtitles-to-your-media/
[plex-subs]: https://support.plex.tv/articles/subtitle-search/
[plex-multiversion]: https://support.plex.tv/articles/200381043-multi-version-movies/
[plex-editions]: https://support.plex.tv/articles/multiple-editions/
[plex-extras]: https://support.plex.tv/articles/local-files-for-trailers-and-extras/
[plex-cinema]: https://support.plex.tv/articles/202934883-cinema-trailers-extras/
[plex-preview]: https://support.plex.tv/articles/202197528-video-preview-thumbnails/
[plex-skip]: https://support.plex.tv/articles/skip-content/
[plex-dvr]: https://support.plex.tv/articles/226463767-frequently-asked-questions-dvr-live-tv/
[plex-watch-together]: https://support.plex.tv/articles/watch-together/
[plex-optimizer]: https://support.plex.tv/articles/214079318-media-optimizer-overview/
[emby-releases]: https://github.com/MediaBrowser/Emby.Releases/releases
[emby-4830]: https://github.com/MediaBrowser/Emby.Releases/releases/tag/4.8.3.0
[emby-4760]: https://github.com/MediaBrowser/Emby.Releases/releases/tag/4.7.0.60
[emby-premiere]: https://emby.media/support/articles/Emby-Premiere.html
[emby-matrix]: https://emby.media/support/articles/Premiere-Feature-Matrix.html
[emby-users]: https://emby.media/support/articles/Users.html
[emby-connect]: https://emby.media/support/articles/Emby-Connect.html
[emby-parental]: https://emby.media/support/articles/Parental-Controls.html
[emby-home]: https://emby.media/support/articles/apps/common/User-Prefs-HomeScreen.html
[emby-collections]: https://emby.media/support/articles/Collections.html
[emby-sync]: https://emby.media/support/articles/Sync.html
[emby-playback]: https://emby.media/support/articles/DirectPlay-Stream-Transcoding.html
[emby-subtitles]: https://emby.media/support/articles/Subtitles.html
[emby-auto-subs]: https://emby.media/support/articles/Automatic-Subtitle-Downloads.html
[emby-movie-naming]: https://emby.media/support/articles/Movie-Naming.html
[emby-trailers]: https://emby.media/support/articles/Trailers.html
[emby-cinema]: https://emby.media/support/articles/Cinema-Intros.html
[emby-intro]: https://emby.media/support/articles/Intro-Skip.html
[emby-livetv]: https://emby.media/support/articles/Live-TV.html
[emby-webhooks]: https://emby.media/support/articles/Webhooks.html
