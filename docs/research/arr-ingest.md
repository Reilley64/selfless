# Radarr and Sonarr webhooks and NFO metadata

Research for [SELFLESS-10](https://app.plane.so/reilley/browse/SELFLESS-10/). Researched on 2026-10-29.

## Summary

Radarr and Sonarr send one JSON document per event to a URL you configure, by `POST` (default) or `PUT`. Movie and TV payloads both carry `eventType`, `instanceName` and `applicationUrl`. The library events are file imported (`Download`), media added, media deleted, file deleted and `Rename`. Each payload carries the app's own ids, the TMDB, IMDb or TVDB ids, the folder and file paths, quality, and a small amount of metadata. That metadata is title, year, genres, image URLs and tags, plus the overview for movies and for episodes.

The only authentication is optional HTTP Basic credentials and optional custom headers. Radarr and Sonarr send the Basic header on the first request. There is no signature, no timestamp, no delivery id and no retry. If a send fails, the error is logged and the event is gone. If sends keep failing for more than 5 minutes, the app skips the webhook for up to an hour at a time, and events in that window are never sent.

The NFO files come from the "Kodi (XBMC) / Emby" metadata option, which is off by default. Radarr writes one Kodi movie NFO per file. Sonarr writes `tvshow.nfo` per series and one NFO per episode file. A file that holds several episodes gets several `<episodedetails>` roots in one NFO. The NFOs hold the rich metadata that webhooks lack: plot, ratings, certification, cast, studio or network, premiere dates, the ids as `<uniqueid>`, and media stream details. They are not always well-formed XML. Images are plain JPEG or PNG files next to the media, named after their type, such as `poster.jpg`, `fanart.jpg`, `season01-poster.jpg` and `<episode file>-thumb.jpg`.

For the first import and for recovery, the APIs (`/api/v3`, with an `X-Api-Key` header) list everything: `GET /api/v3/movie` returns every movie with its file, and Sonarr needs `GET /api/v3/series` followed by `episode` and `episodefile` calls for each series. History and webhooks both miss whole classes of change, such as files found by a disk scan and media deleted from the app. So the design that holds up treats each webhook as a hint to re-sync one movie or series, and runs a full reconcile on startup and on a timer. A later grilling ticket owns that decision.

## Versions read

All claims come from source code at these refs, unless a line cites something else.

| Project | Stable read | Released | Development read |
| --- | --- | --- | --- |
| Radarr | [`v6.4.4.10685`](https://github.com/Radarr/Radarr/releases/tag/v6.4.4.10685) | 2026-09-16 | `develop` at [`c0a0be6`](https://github.com/Radarr/Radarr/tree/c0a0be6dde11ec4c9576366a7623322ea2a08775) (2026-10-27). The latest prerelease is v7.0.0.10815 (2026-10-25). |
| Sonarr | [`v4.0.20.3014`](https://github.com/Sonarr/Sonarr/releases/tag/v4.0.20.3014) | 2026-09-16 | `develop` at [`58fe696`](https://github.com/Sonarr/Sonarr/tree/58fe69664800283237cbca06ace87766dfc66615) (2026-10-27). The latest prerelease is v5.0.0.2838 (2026-10-28). |

Both projects are GPL-3.0, so selfless should read them for behaviour and copy no code. The Servarr wiki ([Servarr/Wiki at `f0dd056`](https://github.com/Servarr/Wiki/tree/f0dd05691d1a3ee92255d9c0342e2d0b0d0d006a)) says little about webhooks beyond the trigger list, so the source is the main reference. The development branches change the webhook payloads only in small, additive ways, listed below.

## Webhooks

### Transport

- The URL, method, username, password and headers are the only settings. The method is `POST` (1) or `PUT` (2), and `POST` is the default ([settings](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookSettings.cs#L28-L40), [method enum](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookMethod.cs)).
- Each send is one request with `Content-Type: application/json` and `Accept: application/json`. The body is the serialized payload ([Radarr proxy](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookProxy.cs#L23-L51), [Sonarr proxy](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookProxy.cs#L23-L51)).
- The JSON uses camelCase property names, omits null values, and is indented. Dates are UTC ([serializer settings](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Common/Serializer/Newtonsoft.Json/Json.cs#L23-L32)). Enums are camelCase strings, except `eventType`, which keeps PascalCase values such as `MovieAdded` ([Radarr event types](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookEventType.cs#L8-L21), [Sonarr event types](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookEventType.cs#L8-L21)).
- Any response with status 400 or higher counts as a failure ([HttpClient](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Common/Http/HttpClient.cs#L106), [HasHttpError](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Common/Http/HttpResponse.cs#L54)). The request times out after 100 seconds unless the request sets its own timeout ([dispatcher](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Common/Http/Dispatchers/ManagedHttpDispatcher.cs#L75-L76)).

### Authentication

- If a username or password is set, the app adds `Authorization: Basic base64(user:pass)` to the first request. It does not wait for a 401 challenge ([proxy](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookProxy.cs#L23-L51), [dispatcher](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Common/Http/Dispatchers/ManagedHttpDispatcher.cs#L81-L86)).
- Each configured header is added as given. A user can set `Authorization: Bearer <secret>` or any custom header this way ([proxy](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookProxy.cs#L23-L51)). The headers field is marked "advanced" in the UI ([settings](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookSettings.cs#L28-L40)).
- Nothing else exists. There is no HMAC signature, no timestamp and no event id, so the receiver cannot detect replays or duplicates from the request alone.
- Basic credentials are base64, not encrypted. Over plain HTTP on the LAN anyone on the path can read them.

### Delivery guarantees

- The webhook runs inside the event handler of the operation that caused it. For example, the import's own thread sends the file-imported webhook ([NotificationService handlers](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/NotificationService.cs#L17-L32)). A slow receiver slows the import, up to the 100 second timeout.
- A failed send is caught, logged and recorded as a failure on the connection. Nothing retries it and nothing queues it ([example handler](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/NotificationService.cs#L123)).
- The connection status backs off. The first failure starts a 5 minute grace period. After that, each failure raises the level and disables the connection for 1, 5, 15, 30 and then 60 minutes, capped at 60 ([NotificationStatusService](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/NotificationStatusService.cs#L18-L19), [backoff periods](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/ThingiProvider/Status/EscalationBackOff.cs), [RecordFailure](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/ThingiProvider/Status/ProviderStatusServiceBase.cs#L85-L136)). While disabled, the connection is skipped for every event ([FilterBlockedNotifications](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/NotificationFactory.cs#L163-L175)). Each success lowers the level by one.
- The result is at-most-once delivery. A server that is down for maintenance loses every event in that window, and can lose more after it is back up while the backoff runs out.

### Events

Each connection has one checkbox per trigger ([Radarr definition](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/NotificationDefinition.cs), [Sonarr definition](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/NotificationDefinition.cs), [wiki trigger list](https://github.com/Servarr/Wiki/blob/f0dd05691d1a3ee92255d9c0342e2d0b0d0d006a/radarr/settings.md#L887-L901)). Every payload has `eventType`, `instanceName` and `applicationUrl` ([base payload](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookPayload.cs)). `applicationUrl` is a user setting and can be empty.

| `eventType` | Radarr trigger | Sonarr trigger | Fires when | Main fields |
| --- | --- | --- | --- | --- |
| `Test` | Test button | Test button | The user clicks Test, or saves the connection without `forceSave` ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Api.V3/ProviderControllerBase.cs#L81-L87)) | Fake data with `id` 1 and "Test Title" ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookBase.cs#L206)) |
| `Grab` | On Grab | On Grab | A release is sent to the download client | Media, `release`, `downloadClient`, `downloadId` |
| `Download` | On File Import, On File Upgrade | On File Import, On File Upgrade | A downloaded file is imported. It fires once per file. | Media, file, `isUpgrade`, `deletedFiles`, `downloadId` ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookImportPayload.cs), [Sonarr](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookImportPayload.cs)) |
| `Download` (import complete) | none | On Import Complete | Sonarr finishes importing every file of one download, such as a season pack | `series`, `episodes`, `episodeFiles`, `sourcePath`, `destinationPath` ([payload](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookImportCompletePayload.cs), [builder](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookBase.cs#L87-L107)) |
| `Rename` | On Rename | On Rename | Files are renamed | Media, renamed files with `previousPath` and `previousRelativePath` ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookRenamedMovieFile.cs#L14-L15)) |
| `MovieAdded` / `SeriesAdd` | On Movie Added | On Series Add | Media is added to the app | `movie` and `addMethod` (`manual`, `list` or `collection`) ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookAddedPayload.cs)), or `series` |
| `MovieDelete` / `SeriesDelete` | On Movie Delete | On Series Delete | Media is removed from the app | Media and `deletedFiles` (bool) ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookMovieDeletePayload.cs), [Sonarr](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookSeriesDeletePayload.cs)) |
| `MovieFileDelete` / `EpisodeFileDelete` | On Movie File Delete, On Movie File Delete For Upgrade | On Episode File Delete, On Episode File Delete For Upgrade | A file is deleted | Media, file, `deleteReason` ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookMovieFileDeletePayload.cs), [Sonarr](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookEpisodeDeletePayload.cs)) |
| `Health`, `HealthRestored` | On Health Issue / Restored | Same | A health check fails or recovers | `level`, `message`, `type`, `wikiUrl` |
| `ApplicationUpdate` | On Application Update | Same | The app updates itself | `previousVersion`, `newVersion` |
| `ManualInteractionRequired` | On Manual Interaction Required | Same | A download needs a manual import | Media, download status messages |

`deleteReason` is one of `missingFromDisk`, `manual`, `upgrade`, `noLinkedEpisodes` or `manualOverride` ([enum](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/MediaFiles/DeleteMediaFileReason.cs)).

Sonarr labels both its per-file payload and its import-complete payload as `Download` ([builder](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookBase.cs#L87-L107)). The receiver must tell them apart by shape: `episodeFile` for one file, `episodeFiles` for the whole download.

Two gaps matter for building a library:

- **Files found without a download send nothing.** The file-imported handlers return early unless the import is a new download ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/NotificationService.cs#L129-L155), [Sonarr](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/NotificationService.cs#L171-L197)). A disk scan imports with `newDownload = false` ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/MediaFiles/DiskScanService.cs#L143), [Sonarr](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/MediaFiles/DiskScanService.cs#L143)). So does a manual import of a file that is already in the media folder ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/MediaFiles/MovieImport/Manual/ManualImportService.cs#L470)). A library import of existing folders sends `MovieAdded` per movie ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/NotificationService.cs#L190)) or `SeriesAdd` after the first scan ([Sonarr](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Tv/SeriesScannedHandler.cs#L49), [handler](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/NotificationService.cs#L428)), but no event for the files.
- **Metadata and path changes send nothing.** No trigger exists for a metadata refresh, a changed title or overview, new episodes appearing on TVDB, or a series moved to a new folder. Only renames have an event.

### Payload contents

Radarr `movie` object ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookMovie.cs#L12-L24)): `id`, `title`, `year`, `releaseDate` (`yyyy-MM-dd`), `folderPath`, `tmdbId`, `imdbId`, `overview`, `genres`, `images`, `tags` (labels, not ids) and `originalLanguage`. The `filePath` field is declared but the payload builders do not set it. `releaseDate` is the physical release date. If that is unknown, it is the cinema date plus 90 days ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Movies/MovieMetadata.cs#L77-L79)).

Radarr `movieFile` object ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookMovieFile.cs#L34-L47)): `id`, `relativePath`, `path`, `quality`, `qualityVersion`, `releaseGroup`, `sceneName`, `indexerFlags`, `size`, `dateAdded`, `languages`, `mediaInfo`, and on imports `sourcePath`. On upgrades, `deletedFiles` entries carry `recycleBinPath` ([builder](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookBase.cs#L52-L85)). `mediaInfo` has `audioChannels`, `audioCodec`, `audioLanguages`, `height`, `width`, `subtitles`, `videoCodec`, `videoDynamicRange` and `videoDynamicRangeType` ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookMovieFileMediaInfo.cs#L27-L35)).

Sonarr `series` object ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookSeries.cs#L10-L23)): `id`, `title`, `titleSlug`, `path`, `tvdbId`, `tvMazeId`, `tmdbId`, `imdbId`, `type` (`standard`, `daily` or `anime`), `year`, `genres`, `images`, `tags` and `originalLanguage`. It has no overview, status, network or certification. Sonarr `episodes` items ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookEpisode.cs#L25-L33)): `id`, `episodeNumber`, `seasonNumber`, `title`, `overview`, `airDate`, `airDateUtc`, `seriesId` and `tvdbId`. Sonarr `episodeFile` matches Radarr's `movieFile`, without `indexerFlags` ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Notifications/Webhook/WebhookEpisodeFile.cs#L33-L45)).

`images` items have `coverType`, `url` and `remoteUrl` ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Notifications/Webhook/WebhookImage.cs)). `remoteUrl` points at the metadata provider. `url` is a path on the app itself, such as `/MediaCover/12/poster.jpg?h=…` in Radarr or `?lastWrite=…` in Sonarr, with no host ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/MediaCover/MediaCoverService.cs#L73-L96), [Sonarr](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/MediaCover/MediaCoverService.cs#L73-L99)). Cover types are `poster`, `banner`, `fanart`, `screenshot`, `headshot` and `clearlogo` ([enum](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/MediaCover/MediaCover.cs#L6-L14)).

Paths are as the app sees them. When Radarr runs in a container with `/movies` mounted, the payload says `/movies/...`, whatever the host path is.

A trimmed Radarr `Download` payload, built from the classes above (values are illustrative):

```json
{
  "movie": {
    "id": 42, "title": "Arrival", "year": 2016, "releaseDate": "2017-02-14",
    "folderPath": "/movies/Arrival (2016)", "tmdbId": 329865, "imdbId": "tt2543164",
    "overview": "...", "genres": ["Drama", "Science Fiction"],
    "images": [{"coverType": "poster", "url": "/MediaCover/42/poster.jpg?h=...", "remoteUrl": "https://image.tmdb.org/..."}],
    "tags": [], "originalLanguage": {"id": 1, "name": "English"}
  },
  "remoteMovie": {"tmdbId": 329865, "imdbId": "tt2543164", "title": "Arrival", "year": 2016},
  "movieFile": {
    "id": 77, "relativePath": "Arrival (2016) Bluray-1080p.mkv",
    "path": "/movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv",
    "quality": "Bluray-1080p", "qualityVersion": 1, "size": 9876543210,
    "dateAdded": "2026-10-29T10:00:00Z", "languages": [{"id": 1, "name": "English"}],
    "mediaInfo": {"videoCodec": "x264", "width": 1920, "height": 800, "audioCodec": "DTS", "audioChannels": 5.1},
    "sourcePath": "/downloads/complete/Arrival.2016.1080p.BluRay.x264/arrival.mkv"
  },
  "isUpgrade": false,
  "downloadClient": "SABnzbd", "downloadClientType": "Sabnzbd", "downloadId": "SABnzbd_nzo_abc",
  "customFormatInfo": {"customFormats": [], "customFormatScore": 0},
  "release": {"releaseTitle": "Arrival.2016.1080p.BluRay.x264", "indexer": "...", "size": 9876543210, "indexerFlags": []},
  "eventType": "Download", "instanceName": "Radarr", "applicationUrl": ""
}
```

### Changes on the development branches

- Radarr `develop` (v7) leaves the payload classes as they are. It only changes how media info fills `audioCodec`, `audioLanguages` and `subtitles` (it reads them from per-stream data).
- Sonarr `develop` (v5) adds `malIds`, `aniListIds` and `originalCountry` to `series` ([source](https://github.com/Sonarr/Sonarr/blob/58fe69664800283237cbca06ace87766dfc66615/src/NzbDrone.Core/Notifications/Webhook/WebhookSeries.cs#L18)). It adds `finaleType` to episodes ([source](https://github.com/Sonarr/Sonarr/blob/58fe69664800283237cbca06ace87766dfc66615/src/NzbDrone.Core/Notifications/Webhook/WebhookEpisode.cs#L35)) and `indexerFlags` to `release`.
- Sonarr v5 adds a new `/api/v5` with the notification endpoints renamed to `/api/v5/connection` ([OpenAPI](https://github.com/Sonarr/Sonarr/blob/58fe69664800283237cbca06ace87766dfc66615/src/Sonarr.Api.V5/openapi.json)). `/api/v3` is still present on `develop`.

## NFO and image files

### When they are written

- Both apps ship the "Kodi (XBMC) / Emby" metadata option, and it is disabled until the user enables it ([Radarr factory](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/MetadataFactory.cs#L33), [Sonarr factory](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/MetadataFactory.cs#L33), [wiki](https://github.com/Servarr/Wiki/blob/f0dd05691d1a3ee92255d9c0342e2d0b0d0d006a/radarr/settings.md#L916)). The API can enable it with `PUT /api/v3/metadata/{id}` ([Radarr OpenAPI](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Api.V3/openapi.json)).
- On a new download, Radarr writes the movie NFO before it publishes the import event. So the NFO exists by the time the `Download` webhook leaves ([ImportApprovedMovie](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/MediaFiles/MovieImport/ImportApprovedMovie.cs#L170-L174)). Sonarr writes the episode NFO and thumbnail on import ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/MetadataService.cs#L143)). Sonarr writes `tvshow.nfo` and series images when it creates the series folder or scans the series. It skips them on episode import ([folder](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/MetadataService.cs#L158), [scan](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/MetadataService.cs#L85), [skip on import](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L143)).
- Images are written when the series or movie folder is created, after a disk scan, and when cached covers change ([Radarr ExtraService](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/ExtraService.cs#L109-L133)).
- A metadata refresh runs a disk scan by default ([refresh](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Movies/RefreshMovieService.cs#L176-L201), [default "Always"](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Configuration/ConfigService.cs#L307)), and the scan rewrites NFOs. Radarr refreshes every 24 hours ([task](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Jobs/TaskManager.cs#L89)) and Sonarr every 12 hours ([task](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Jobs/TaskManager.cs#L103)). The app skips the write when the SHA-256 of the new content matches the stored hash ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/MetadataService.cs#L236-L249)). So a changed NFO on disk means changed metadata, and no webhook reports it.
- The app reads back `<watched>true</watched>` from the old NFO and keeps it ([Radarr](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L481)). This is the only state it preserves from the old file.

### Radarr movie NFO

The name is `<movie file name>.nfo`, or `movie.nfo` if the user picks that option ([filename](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L469-L477), [settings defaults](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadataSettings.cs#L17-L24)). The root is `<movie>`. Elements ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L118-L445)):

- `title` in the configured metadata language (default English), from Radarr's translations ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L126)), plus `originaltitle` and `sorttitle`.
- `ratings` with `rating` children named `imdb`, `themoviedb` and `tomatometerallcritics`, each with `value` and `votes`. Also `rating` (TMDB value) and `criticrating` (Rotten Tomatoes).
- `plot`, `runtime` (minutes), `mpaa` (certification), `premiered` (in-cinemas date), `year`, `studio`, `status`, `genre` (repeated), `tag` (repeated, Radarr tag labels) and `set` with a `tmdbcolid` attribute and `name` (the collection).
- `uniqueid type="tmdb" default="true"` and `uniqueid type="imdb"` ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L260)), plus the legacy `id` (TMDB).
- `thumb aspect="poster"` and `fanart/thumb`, which hold remote TMDB URLs, not local files.
- `credits` (screenplay), `director`, and `actor` with `name`, `role`, `order` and a remote `thumb` ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L397)).
- `trailer` as a Kodi YouTube plugin URL, and `watched`.
- `fileinfo/streamdetails` with `video` (`aspect`, `bitrate`, `codec`, `framerate`, `height`, `width`, `scantype`, `duration`, `durationinseconds`, `hdrtype`), one `audio`, and `subtitle` languages. On `develop`, Radarr writes one `audio` element per stream ([source](https://github.com/Radarr/Radarr/blob/c0a0be6dde11ec4c9576366a7623322ea2a08775/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L374)).
- Several elements are always empty: `userrating`, `top250`, `outline`, `tagline`, `country`, `playcount` and `lastplayed`.

If "Movie Metadata URL" is on, Radarr appends the TMDB and IMDb URLs as plain text lines after the XML ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L434-L439)). That file is not well-formed XML.

### Sonarr series and episode NFOs

`tvshow.nfo` in the series folder has the root `<tvshow>` ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L141-L275)). It holds `title`, `rating` (if votes > 0), `plot`, `mpaa`, `id` (TVDB), `uniqueid` for `tvdb` (default), `imdb`, `tmdb` and `tvmaze`, plus `genre`, `tag`, `status`, `premiered`, `enddate` (ended series only), `studio` (network) and `actor` (`name`, `role` and a remote `thumb`). An optional `episodeguide` holds a JSON object of ids ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/KodiEpisodeGuide.cs)). An optional TVDB URL line can follow the XML, with the same parsing problem as Radarr ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L265-L272)). Sonarr writes no season NFO.

`<episode file name>.nfo` holds one `<episodedetails>` per episode in the file ([loop](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L301)). Sonarr writes it as an XML fragment ([writer settings](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L293)), so a multi-episode file has several roots. Kodi documents the same layout for multi-episode NFOs ([Kodi wiki](https://kodi.wiki/view/NFO_files/Episodes)). Each element ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L278-L400)) has:

- `title`, `season`, `episode`, `aired`, `plot`, and for specials `displayafterseason`, or `displayseason` and `displayepisode`.
- `uniqueid type="tvdb" default="true"` and `uniqueid type="sonarr"` holding Sonarr's episode id ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L327)). The Sonarr id links an NFO to the webhook and API records without any guessing.
- `thumb`, which is empty or a remote URL, then `watched`, `rating`, and `fileinfo/streamdetails` like Radarr's. `develop` adds `hdrtype` and one `audio` per stream ([source](https://github.com/Sonarr/Sonarr/blob/58fe69664800283237cbca06ace87766dfc66615/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L368)).

### Image files

| App | File | Source |
| --- | --- | --- |
| Radarr | `poster.jpg`, `fanart.jpg`, other types as `<type>.jpg`, with `clearlogo.png` the one exception, in the movie folder | Copied from Radarr's cover cache ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L458-L465)) |
| Sonarr | `poster.jpg`, `banner.jpg`, `fanart.jpg` (and other types) in the series folder | Copied from the cover cache ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L461-L492)) |
| Sonarr | `season01-poster.jpg`, `season01-banner.jpg`, `season-specials-poster.jpg` in the series folder | Downloaded from the remote URL ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L461-L492)) |
| Sonarr | `<episode file name>-thumb.jpg` next to the episode | The first episode's screenshot, downloaded ([source](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadata.cs#L423)) |

The Sonarr defaults turn on every file type except the remote thumb URL inside the episode NFO ([settings](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadataSettings.cs#L16-L24)). The Radarr defaults turn on the NFO and images, add the collection name, and leave the URL lines off ([settings](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcMetadataSettings.cs#L17-L24)).

To detect an existing Kodi NFO, Radarr and Sonarr match the regex `<(movie|tvshow|episodedetails|artist|album|musicvideo)>` on files up to 10 MB ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Extras/Metadata/Consumers/Xbmc/XbmcNfoDetector.cs#L16)). A selfless parser should be at least as tolerant: wrap the content in a synthetic root, and cut any text after the last closing tag.

## First import and recovery through the APIs

### API basics

- Authenticate with the `X-Api-Key` header or the `apikey` query parameter ([auth setup](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Http/Authentication/AuthenticationBuilderExtensions.cs#L53-L61), [Radarr OpenAPI](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Api.V3/openapi.json), [Sonarr OpenAPI](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/Sonarr.Api.V3/openapi.json)).
- Radarr and Sonarr ship an OpenAPI document for each API version in the repo. Use those as the contract, not the rendered docs sites. The docs sites are JavaScript apps that return no content without a browser.
- `GET /api/v3/system/status` returns the version, which lets selfless pick between `/api/v3` and Sonarr's new `/api/v5`.

### Inventory calls

| Need | Radarr | Sonarr |
| --- | --- | --- |
| All media | `GET /api/v3/movie`. Each item embeds `movieFile`, because the repository joins the file ([controller](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Api.V3/Movies/MovieController.cs#L115), [join](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/Movies/MovieRepository.cs#L61), [resource](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Api.V3/Movies/MovieResource.cs#L83)). | `GET /api/v3/series`. Items have `statistics` with file counts and size, but no episodes. |
| Files | Already in the movie list. `GET /api/v3/moviefile?movieId=` for one movie. | `GET /api/v3/episodefile?seriesId=`. It needs `seriesId` or `episodeFileIds` ([controller](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/Sonarr.Api.V3/EpisodeFiles/EpisodeFileController.cs#L60-L64)). |
| Episodes | not applicable | `GET /api/v3/episode?seriesId=` ([controller](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/Sonarr.Api.V3/Episodes/EpisodeController.cs#L28)) |
| NFO and image paths | `GET /api/v3/extrafile?movieId=` lists metadata, subtitle and other files with `relativePath` and `type` ([controller](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Api.V3/ExtraFiles/ExtraFileController.cs#L27)) | No such endpoint in v4. Derive paths from the naming rules above. |
| Changes since a time | `GET /api/v3/history/since?date=` | `GET /api/v3/history/since?date=` |
| Webhook setup | `GET/POST/PUT /api/v3/notification` | Same on v3. `/api/v5/connection` on v5. |
| Turn on Kodi metadata | `GET/PUT /api/v3/metadata` | Same |

A first import for N series costs 1 + 2N Sonarr calls, plus one Radarr call. For a household library that is a few hundred requests to a LAN service.

### Why history alone cannot recover missed webhooks

- History records an import only when it is a new download, the same filter the webhook uses ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/History/HistoryService.cs#L180)). Disk-scan imports leave no history.
- Deleting a movie deletes its history rows ([source](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Core/History/HistoryService.cs#L296-L298)). History never shows that a movie or series left.
- Radarr's history event types are `grabbed`, `downloadFolderImported`, `downloadFailed`, `movieFileDeleted`, `movieFolderImported`, `movieFileRenamed` and `downloadIgnored`. Sonarr has the matching set with `seriesFolderImported` and `episodeFile*` ([Radarr OpenAPI](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Api.V3/openapi.json), [Sonarr OpenAPI](https://github.com/Sonarr/Sonarr/blob/v4.0.20.3014/src/Sonarr.Api.V3/openapi.json)). None covers adding media or a metadata change.

So `history/since` is a cheap way to catch missed download imports, deletes and renames. Only a full diff of the inventory against selfless's own records catches everything.

### Recovery approach the evidence supports

1. Answer each webhook with 2xx right away and queue the work. The app waits on the request inside its import thread, and failures trip the backoff.
2. Treat a webhook as "this movie or series changed". Use the `id` from the payload to fetch that one item from the API, then read its NFO and images from disk. The re-sync is idempotent, so duplicate or reordered webhooks do no harm.
3. Run a full reconcile when selfless starts, after any period when it could not receive webhooks, and on a timer. The reconcile compares the API inventory by app id and file id against what selfless has, adds and removes items, and re-reads NFOs whose modification time or hash changed. The timer only needs to match the apps' own 12 or 24 hour refresh cycle to pick up NFO rewrites.
4. A Radarr reconcile is one request. A Sonarr reconcile can skip the per-series calls when `statistics` (`episodeFileCount`, `sizeOnDisk`) and the series' `tvshow.nfo` hash have not changed.

Both apps also run a SignalR hub at `/signalr/messages`, authenticated with the same API key as `access_token` ([Startup](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/NzbDrone.Host/Startup.cs#L297), [auth setup](https://github.com/Radarr/Radarr/blob/v6.4.4.10685/src/Radarr.Http/Authentication/AuthenticationBuilderExtensions.cs#L53-L61)). The web UI uses it for live updates. It is not documented as a public API, so this report does not recommend it.

## Open questions for the grilling ticket

1. Is API data "metadata"? The brief says metadata comes only from NFO files or webhook data. The recovery plan needs the API for the inventory (ids, paths, files). The decision is whether the API may only say *what exists* while the NFO says *what it is*, or whether API fields such as `overview` may fill gaps.
2. Who turns on Kodi metadata in Radarr and Sonarr? It is off by default. Options are a setup step for the user, or selfless enabling it through `PUT /api/v3/metadata/{id}` with a stored API key.
3. Who creates the webhook connection? The same choice exists: manual setup, or selfless registering itself with `POST /api/v3/notification`. Self-registration needs selfless to store an API key for each instance.
4. What webhook secret? Options are Basic credentials, a bearer token in a custom header, or a secret path segment. Basic is the only option in the main settings form, and every option is plain text on HTTP.
5. How are paths mapped? Payload and API paths are the app's container paths. selfless needs a per-instance prefix map, or the same mount points.
6. Which ids are the keys? The candidates are the app ids (`movie.id`, `series.id`, `episode.id`, file ids), which are stable within one instance, and TMDB or TVDB ids, which are stable across a reinstall. Supporting more than one instance (a separate 4K Radarr, say) affects this choice, and `instanceName` tells instances apart.
7. How does selfless notice NFO rewrites? Options are a filesystem watcher, an mtime scan inside the reconcile, or both.
8. How should parsing handle broken NFOs (trailing URL lines, multi-root episode files)? Also, should `<thumb>` remote URLs be used, or only the image files on disk?
9. When should selfless target Sonarr v5 and `/api/v5`? It is in prerelease as of 2026-10-28.
10. Should `Grab` events show "downloading" items in the UI, or should selfless ignore all events that are not about the library?
