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
# [![Watch on Youtube](http://i3.ytimg.com/vi/2B49ljE7pGM/hqdefault.jpg)](https://www.youtube.com/watch?v=2B49ljE7pGM)
+++

# Video Announcement

# Transcript

Welcome Fighters Anthologists to the first, 0.1, release video of OpenFA

For those that don't know, OpenFA is a black-box re-implementation of the Janes
Fighters Anthology engine, derived wholely from inspection of the game assets and
observation of the game's behavior.

In this release we are switching to numbered versions, instead of the previous
date-based format. That's because this release is for the first milestone on our
roadmap: flight. OpenFA is a proper flight sim now and has flown (rather slowly)
out of testing and into alpha.

Those of you who downloaded one of the prior releases will be familiar with the
current, default view. Since I don't know how MNU or DLG files work, there's
nothing to load by default. So instead we boot into the famous, instantly
recognizable, vista you're seeing currently: Mt Everest. ... Or at least that's
what you would be seeing if I had loaded the 2.5TiB of terrain LIBs. We'll get
back to that in a moment; for now, let's load up a mission the fun way.

OpenFA borrows the long tradition of 90's classics by including a drop-down
console that lets us inspect and modify the running game state at will.  It's
accessed, in the Quake 3 style with Ctrl+tilde.  If we run list() in here, we
can see that all that's currently loaded are the core game resources. Diving in,
we can list the functionality available in those resources like this:
`game.list()`. Let's use `game.load_mission("~U02.M")` to open up a mission we
can fly around in. And since HUDs are not currently implemented, we also need to
switch to the external camera view with: `@camera.controller.set_mode("external")`.
Tab completion works on all of this, so if you generally only have to type the
first few letters and hit `tab` to fill in the rest.

So, I expected that implementing the flight model would take about a month. That
was six months ago. It actually took closer to two. It's based on
David Allerton's "Principles of Flight Simulation", which I chose because
apparently that's what DCS uses. And it was great! It felt super realistic and
fun to fly; almost as good as DCS, even.

And it felt nothing _at all_ like Fighters Anthology.

So I deleted it and started over. I spent the next three months of evenings
between Fighters Anthology and Google Sheets trying to figure out how Fighers
Anthology really works. I'd say I'm like 60% there. What you're seeing on screen
now is what I built based on that learning. It's still built on Allerton's
framework, but feels _much closer_ to Fighters Anthology's flight model.

The release is available to download at gitlab.com/terrence_too/openfa, or on
the new openfa.org; links in the description. Go check it out if you have the
one of the Janes Fighters Anthology games and want to play around more with what
was shown off here. Our next roadmap item is going to be making the terrain
interactive and just generally more... there. In the meantime, don't forget to
like and subscribe here and on openfa.org for updates.