# selfless

A self-hosted media server for one household. It plays the movies and shows that Radarr and Sonarr manage.

## Library

**Library**:
Every movie and show selfless knows about, as reported by Radarr and Sonarr.
_Avoid_: Collection, catalog

**Movie**:
A single film in the library, managed by Radarr.
_Avoid_: Film

**Show**:
A television program in the library, managed by Sonarr, made of seasons.
_Avoid_: Series, TV series, program

**Season**:
A numbered group of episodes within a show.

**Episode**:
A single installment of a show, belonging to one season.

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
The movies and episodes a user has a resume position in.

**Next up**:
For each show a user is part way through, the first episode after the last one they watched.
_Avoid_: On deck, up next

**Popular now**:
The movies and shows the most different users have played in the last 14 days.
_Avoid_: Trending
