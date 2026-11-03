# selfless

A self-hosted media server for one household. It plays the movies and shows that Radarr and Sonarr manage.

## Library

**Library**:
Every movie and episode that has a file on disk, as reported by Radarr and Sonarr, and the shows and seasons they belong to.
_Avoid_: Collection, catalog

**Movie**:
A single film in the library, managed by Radarr.
_Avoid_: Film

**Show**:
A television program in the library, managed by Sonarr, made of seasons.
_Avoid_: Series, TV series, program

**Season**:
A numbered group of episodes within a show.

**Specials**:
The season numbered 0, holding a show's episodes that sit outside its numbered seasons. It is listed after the numbered seasons.
_Avoid_: Season 0, extras

**Episode**:
A single installment of a show, belonging to one season.

**Media file**:
The video file on disk that a movie or episode plays from. One media file can hold several episodes.
_Avoid_: File, media source, version, asset

**Genre**:
A category from the NFO files that users filter movies and shows by.
_Avoid_: Tag, category

## Library sync

**Source**:
Radarr or Sonarr, as the system a movie or show comes from. Each source has its own webhook password, reconcile and sync status.
_Avoid_: Provider, arr, upstream

**Webhook delivery**:
One webhook request from a source, kept in the inbox until selfless has re-synced the movie or show it names.
_Avoid_: Event, notification, hook

**Reconcile**:
A full comparison of one source's movies or shows with the library. It adds, re-syncs and removes items. It runs at startup, every reconcile interval, and when an admin presses Sync now.
_Avoid_: Full sync, scan, refresh

**Held removal**:
A reconcile's removal of more than half of a source's stored items. Nothing is removed until an admin confirms it, and a later reconcile replaces it.
_Avoid_: Pending deletion, quarantine

**Sync status**:
What an admin sees about library sync for each source: the last webhook, rejected webhooks, the last reconcile, any held removal, and the sync problems.
_Avoid_: Health, sync state

**Sync problem**:
A movie or episode selfless could sync only in part, listed in the sync status with the path to fix. It is a missing media file, an unreadable file, an unreadable NFO or an unrecognised media file.
_Avoid_: Error, issue, warning

**Missing media file**:
A sync problem: Radarr or Sonarr reports a media file whose path doesn't exist on selfless's disk. The movie or episode leaves the library until the path appears.
_Avoid_: File not found

**Unreadable file**:
A sync problem: a media file, NFO or image exists on disk but selfless can't read it, for example because of its permissions. The movie, or the whole show, stays as it was until the read succeeds.
_Avoid_: Permission error, IO error

**Unreadable NFO**:
A sync problem: an NFO file selfless read but couldn't parse. Radarr's or Sonarr's title, year, genres and overview stand in for it.
_Avoid_: Bad NFO, invalid metadata

**Unrecognised media file**:
A sync problem: ffprobe couldn't read a media file's container or streams. The movie or episode stays in the library.
_Avoid_: Ffprobe failed, probe error

## People

**User**:
A household member who watches. Users sign in on the web app, and on each TV where they want to appear in the user picker.
_Avoid_: Account, viewer, profile, member

**Colour**:
The colour from a fixed list that marks a user in the apps. It never changes, and two users can share one.
_Avoid_: Avatar colour, profile colour

**Invite**:
An email an admin sends to add a user. Its link lets the person choose their username and password. It works once and expires 7 days after the email is sent.
_Avoid_: Invitation code, sign-up link, registration

**Queued**:
An invite whose email is waiting to be sent. A new or resent invite is queued.
_Avoid_: Pending, sending, unsent

**Sent**:
An invite whose email the mail server accepted. Its link works for 7 days from then.
_Avoid_: Delivered

**Refused**:
An invite whose email the mail server refused for its address. It is not sent again until an admin resends it.
_Avoid_: Bounced, failed, rejected

**Admin**:
An account that runs the server through the admin app. An admin is not a user, cannot watch, and never appears in the user picker.
_Avoid_: Account, administrator, owner

**Admin session**:
An admin's sign-in to the admin app. It lasts until it is ended or unused for 12 hours.
_Avoid_: Admin login

**Client id**:
A random id each app install or browser makes once and sends with every sign-in, so signing in again as the same user from it replaces that user's earlier session from it.
_Avoid_: Device id, install id

**Session**:
A user's sign-in from one browser or one TV app install. It lasts until it is ended or, on the web, unused for 30 days.
_Avoid_: Login, device session

**Client kind**:
Which app a session signed in from: web, Apple TV or Google TV.
_Avoid_: Device type, client type, app type

**Device name**:
The name a session shows for where it signed in, such as "Firefox on macOS" or "Living Room".
_Avoid_: Device label, client name

## TVs

**User picker**:
The first screen of the TV app. It lists the users signed in on that TV and has a button to sign in another user.
_Avoid_: Profile picker, profile switcher

## Watching

**Resume position**:
How far a user got through a movie or episode they have not finished.
_Avoid_: Progress, bookmark, offset

**Watched**:
The state of a movie or episode a user has finished or marked as finished.
_Avoid_: Played, seen, completed

**Watch state**:
What selfless keeps for one user and one movie or episode: its resume position, whether it is watched, its watched at and its last played.
_Avoid_: Progress, play state, user data

**Watched at**:
When a movie or episode last changed from unwatched to watched for a user. Watching it to the end again while it is watched doesn't change it.
_Avoid_: Completed at, finished at

**Continue watching**:
The movies and episodes a user has a resume position in, plus the next up episode of each show they are part way through. A show appears once.

**Continue watching removal**:
A user's choice to leave a movie or show out of their Continue watching until they play it again. A new episode arriving doesn't bring a show back.
_Avoid_: Dismissal, hidden title

**Next up**:
For each show a user is part way through, the first episode after the one that most recently changed from unwatched to watched. Rewatching an episode that is already watched doesn't move it.
_Avoid_: On deck, up next

**Last played**:
When a user last watched past the first minute of a movie or episode. Marking it watched by hand doesn't count.
_Avoid_: Last watched, recently played

**Recently added**:
The movies and episodes whose file Radarr or Sonarr first added most recently. Replacing a file with a better copy does not count as adding it.
_Avoid_: New, latest

**Popular now**:
The movies and shows with the most different users whose last played time is in the last 14 days.
_Avoid_: Trending

**Now playing**:
The movies and episodes users are playing at this moment, and the device each one plays on.
_Avoid_: Active sessions, streams, activity

**Featured title**:
The one movie or show shown large at the top of Home: the top of Popular now, or the latest released title when nobody has played anything.
_Avoid_: Hero, billboard, spotlight

**Playback**:
One device playing one movie or episode, from pressing play until it stops.
_Avoid_: Session, stream, play

**Direct play**:
A playback where the device plays the file as it is on disk.
_Avoid_: Direct stream

**Remux**:
A playback where selfless copies the file's video and audio into a format the device can open, without changing them.
_Avoid_: Direct stream, repackage

**Transcode**:
A playback where selfless converts the video or audio into a format the device can play.
_Avoid_: Convert, encode
