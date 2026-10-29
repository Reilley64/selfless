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

## People

**User**:
A household member who watches. Users sign in on the web app, and on each TV where they want to appear in the user picker.
_Avoid_: Account, viewer, profile, member

**Admin**:
An account that runs the server through the admin app. An admin is not a user, cannot watch, and never appears in the user picker.
_Avoid_: Account, administrator, owner

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

**Continue watching**:
The movies and episodes a user has a resume position in, plus the next up episode of each show they are part way through. A show appears once.

**Next up**:
For each show a user is part way through, the first episode after the last one they watched for the first time. Rewatching an episode doesn't move it.
_Avoid_: On deck, up next

**Last played**:
When a user last watched past the first minute of a movie or episode. Marking it watched by hand doesn't count.
_Avoid_: Last watched, recently played

**Recently added**:
The movies and episodes whose file Radarr or Sonarr first added most recently. Replacing a file with a better copy does not count as adding it.
_Avoid_: New, latest

**Popular now**:
The movies and shows the most different users have played in the last 14 days.
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
