source [file join [file dirname [info script]] view.tcl]

if {![llength [info commands wm]]} {
    puts stderr "The Steelgum preview needs a working Tcl/Tk graphical session."
    exit 1
}

proc canvasDraw {shape args} {
    switch -- $shape {
        poly {
            lassign $args points fill border
            .room create polygon {*}$points -fill $fill -outline $border
        }
        rect {
            lassign $args x1 y1 x2 y2 fill border
            .room create rectangle $x1 $y1 $x2 $y2 -fill $fill -outline $border
        }
        line {
            lassign $args x1 y1 x2 y2 colour
            .room create line $x1 $y1 $x2 $y2 -fill $colour
        }
        text {
            lassign $args x y label colour size weight
            .room create text $x $y -text $label -fill $colour -anchor nw \
                -font [list "DejaVu Sans" -$size $weight]
        }
    }
}

proc closeGame {} {
    catch {fileevent $::session readable {}}
    catch {puts $::session quit}
    catch {close $::session}
    destroy .
}

proc showError {message} {
    puts stderr $message
    tk_messageBox -icon error -type ok -title "Steelgum" -message $message
    closeGame
    exit 1
}

proc response {} {
    if {[catch {
        while {[gets $::session line] >= 0} {
            set previousFrame $::preview::frame
            preview::receive $line
            if {[lindex [split $line] 0] eq "frame"} {
                if {$previousFrame ne $::preview::frame} {
                    .room delete all
                    preview::draw canvasDraw
                }
                set ::awaiting 0
                if {!$::restartRequested && [lindex $::preview::frame 6]} {
                    set ::watching 0
                }
            }
        }
        if {[eof $::session]} {
            fileevent $::session readable {}
            fconfigure $::session -blocking 1
            if {[catch {close $::session} details]} {
                error $details
            }
            error "The Door Game session exited unexpectedly."
        }
    } message]} {
        showError $message
    }
}

proc send {command} {
    if {[catch {puts $::session $command} message]} {
        showError $message
    }
    set ::awaiting 1
}

proc advance {} {
    if {!$::awaiting} {
        if {$::restartRequested} {
            set ::restartRequested 0
            send restart
        } else {
            set x [expr {int($::keys(d) || $::keys(right))-int($::keys(a) || $::keys(left))}]
            set z [expr {int($::keys(s) || $::keys(down))-int($::keys(w) || $::keys(up))}]
            if {$::watching} {
                set x 0
                set z -1
            }
            send "step $x $z"
        }
    }
    after $::preview::stepMs advance
}

proc setKey {key direction} {
    set key [string tolower $key]
    if {![info exists ::keys($key)]} {
        return
    }
    set ::keys($key) $direction
    if {$direction} {
        set ::watching 0
    }
}

proc clearKeys {} {
    foreach key [array names ::keys] {
        set ::keys($key) 0
    }
}

proc watch {} {
    set ::restartRequested 1
    set ::watching 1
}

proc restart {} {
    set ::restartRequested 1
    set ::watching 0
}

proc click {x y} {
    if {$x >= 671 && $x <= 910} {
        if {$y >= 494 && $y <= 527} {
            watch
        } elseif {$y >= 537 && $y <= 567} {
            restart
        }
    }
}

set command [list [lindex $argv 0] --preview-session {*}[lrange $argv 1 end]]
set awaiting 1
set watching 0
set restartRequested 0
array set keys {w 0 a 0 s 0 d 0 up 0 down 0 left 0 right 0}
wm title . "Steelgum — Door Game"
wm resizable . 0 0
wm protocol . WM_DELETE_WINDOW closeGame
canvas .room -width $preview::width -height $preview::height -borderwidth 0 \
    -highlightthickness 0 -background #101720
pack .room
bind . <KeyPress> {setKey %K 1}
bind . <KeyRelease> {setKey %K 0}
bind . <FocusOut> clearKeys
bind . <space> watch
bind . <r> restart
bind . <Escape> closeGame
bind .room <Button-1> {click %x %y}
if {[catch {
    set session [open [list | {*}$command] r+]
    fconfigure $session -blocking 0 -buffering line
    fileevent $session readable response
} message]} {
    showError $message
}
after $preview::stepMs advance
