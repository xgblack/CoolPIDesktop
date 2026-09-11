on run argv
 tell application "System Events"
  tell process "cool-pi-desktop"
   set frontmost to true
   set nodes to entire contents of window 1
   repeat with node in nodes
    try
     if name of node is item 1 of argv then
      click node
      return "clicked " & item 1 of argv
     end if
    end try
   end repeat
   error "UI element not found: " & item 1 of argv
  end tell
 end tell
end run
