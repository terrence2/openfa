# Roadmap
What are the next major features that I plan to work on?

Please keep in mind:
* This is highly aspirational.
* I'll likely be distracted along the way.
* Some of it may be impossible anyway. 

In rough order, but not with deadlines:

1) Classic FA flight model, with feel similar to FA, for most traditional aircraft.
   1) Outcomes:
      * Fly around static maps and missions.
      * An interesting tech demo, usable on more than just the development machine.
   2) Non-goals:
      * Ground interactions
      * Collisions
      * Vectored Thrust
      * AIs or NPCs that fly

2) Fix the terrain engine.
   1) Outcomes:
      * Faster CPU tiles computation
      * Stream tiles from the internet
      * Integrate landsat 8/9 color imagery
      * Ability to query terrain height regions on the CPU
      * Use GDAL to build imagery sets; delete dump-terrain-tiles, with fire
   2) Non-goals: 
      * Collisions with terrain
      * Ground movers

3) Landing and Takeoff
   1) Outcomes:
      * Fly around missions that start on the ground.
      * Touch-and-go's
      * Drive wheeled vehicles around, maybe?
   2) Non-goals:
      * Collisions between shapes
      * Weapons or combat

4) HUD and cockpit view
   1) Outcomes:
      * Something that feels like FA to fly around in
      * May push changes back to flight and ground models
      * Maybe, possibly some relevant cockpit systems displays, but probably not
   2) Non-goals:
      * Working anything beyond the very basic cockpit and hud tape

5) Guns and Damage Model
   1) Outcomes:
      * Ability to fire guns / gun pods and maybe rockets / rocket pods
      * Ability to damage structures and vehicles with rocket and gun fire
      * Show the damage shape or shapes upon destruction
   2) Non-goals:
      * Rendering fire or smoke
      * Guided anything
      * Wind effects
      * Tracking and target prediction
      * Exact conformance to FA behavior
        * The gunplay is great, but the bullet graphics are... sad at this point

6) Research the AI/BI formats
   1) Outcomes:
      * Understand enough of the instruction format to decode all instructions
   2) Non-goals:
      * Working AI

7) AI (Easy)
   1) Outcomes:
      * Have something not static in the world so that missile implementation can be meaningful
      * Very, very basic fly/drive to waypoint steering
      * Maybe point-this-end-at-enemy combat AI, but probably not
   2) Non-goals:
      * Fun combat
      * Working missions

8) Radar, targets, tracking, etc
   1) Outcomes:
      * Ability to turn the radar on and off in air and ground mode
      * More or less correct targets should show up in each
      * Targets should be highlighted in the HUD too
      * Target camera
   2) Non-goals:
      * AWACS
      * IR/FLIR
      * IFF
      * ECS
      * Jamming
      * Chaff/Flares
      * etc

9) Missiles
   1) Outcomes:
      * Ability to lock onto a target and deploy a missile
      * Radar guidance, in some form
      * Ability of AI to deploy missiles
   2) Non-goals:
      * IR missiles
      * Smoke, Fire, or any effects whatsoever outside the missile itself
      * Most missile guidance modes

10) Mission completion checking
    1) Outcomes:
       * Ability to pass missions with basic objectives
    2) Non-goals:
       * MC support
       * Ability to win all possible missions
       * Ability to fail missions

!!! Some FA mission should now be "playable", at least in spirit... maybe !!!

11+. ... The long, long, looooooong tail of work to make campaigns completable.
But this is already like a decade of work; no point planning much further yet.