+++
title = "OpenFA Alpha Release 0.1"
description = "First alpha release of OpenFA"
date = 2022-11-19
draft = false
slug = "openfa-release-1"

[taxonomies]
categories = ["releases"]
tags = ["release", "flight-model", "demo", "video"]

[extra]
comments = false
+++

# Video Announcement
[![Watch on Youtube](http://i3.ytimg.com/vi/50-7lQWRNEs/hqdefault.jpg)](https://www.youtube.com/watch?v=50-7lQWRNEs)

# Transcript

Welcome Fighters Anthologists to the first alpha release of OpenFA!

For those that don't know, OpenFA is a black-box re-implementation of the Janes
Fighters Anthology engine, derived wholely from inspection of the game assets and
observation of the game's behavior.

Why the switch to numbered alpha releases?  Because this is the first milestone
release on our roadmap: flight. OpenFA is an actual flight sim now and has
flown, if slowly, out of testing and into alpha.

Those of you who downloaded one of the prior releases will recognize what you're
seeing currently: OpenFA's rather unhelpful opening screen. Since nobody knows
exactly how MNU or DLG files work, yet, there's nothing to load by default. So
instead we boot into this famous, instantly recognizable, vista: Mt Everest. Or
at least that's what you would be seeing if the terrain LIBs were loaded.  We'll
get back to that in a moment; for now, where we're going, we don't need DLGs or
MNUs.

OpenFA borrows from the long tradition of other 90's classics by including a
console that lets us inspect and mod the game as its running.  Like Quake 3,
it's accessed with Ctrl+tilde (that's the squiggle at the top left, under
Escape). If we run list() in here, we can see that all that's currently loaded
are the core game resources. Diving in, we can list the functionality available
in those resources like this: `game.list()`. Let's use
`game.load_mission("~B21.M")` to open up a mission we can fly around in. And
since HUDs are not currently implemented, we also need to switch to the external
camera view with: `@camera.controller.set_mode("external")`.  Tab completion
works on all of this, so if you generally only have to type the first few
letters and hit `tab` to fill in the rest. Commands are remembered across runs
so you can hit the up key a few times to find old commands to re-run them, even
across sessions.

``` Demo spawning some more objects & fly towards something more interesting ```

So, I expected that implementing the flight model would take about a month. That
was six months ago. It actually took closer to two. It's based on David
Allerton's "Principles of Flight Simulation", which I chose because apparently
that's what DCS uses. And it was great! It felt super realistic and fun to fly;
almost as good as DCS, even.

But it felt nothing _at all_ like Fighters Anthology.

So I deleted it and started over. I spent the next three months of evenings
between Fighters Anthology and Google Sheets trying to figure out _exactly_ how
Fighers Anthology really works. I'd say I'm like 60% there. But as you can see
it more or less works. There's still a lot of bugs and details that aren't quite
as perfect as I'd like yet. And occasionally it just completely whigs out
and does something hilariously bonkers. It's still built on Allerton's
framework, but feels _much closer_ to Fighters Anthology's flight model.

The release is available to download at gitlab.com/terrence_too/openfa, or via
the new openfa.org website; I'll put links in the description. Go check it out
if you have the one of the Janes Fighters Anthology games and want to play
around more with what I showed off here. The next roadmap item is going to
be making the terrain interactive. I want to improve the low end of the
flight model and that's going to be hard if I can't do touch and go's.  In
the meantime, don't forget to like, share, and subscribe for updates.

