# Renders the successful scripted playthrough using the same view as the Tk app.
source [file join [file dirname [info script]] view.tcl]

proc xml {text} {
    return [string map {& &amp; < &lt; > &gt; \" &quot;} $text]
}

proc svgDraw {shape args} {
    switch -- $shape {
        poly {
            lassign $args points fill border
            set pairs {}
            foreach {x y} $points { lappend pairs "$x,$y" }
            puts [format {<polygon points="%s" fill="%s" stroke="%s"/>} \
                [join $pairs " "] $fill $border]
        }
        rect {
            lassign $args x1 y1 x2 y2 fill border
            puts [format {<rect x="%s" y="%s" width="%s" height="%s" fill="%s" stroke="%s"/>} \
                $x1 $y1 [expr {$x2-$x1}] [expr {$y2-$y1}] $fill $border]
        }
        line {
            lassign $args x1 y1 x2 y2 colour
            puts [format {<line x1="%s" y1="%s" x2="%s" y2="%s" stroke="%s"/>} $x1 $y1 $x2 $y2 $colour]
        }
        text {
            lassign $args x y label colour size weight
            puts [format {<text x="%s" y="%s" fill="%s" font-size="%s" font-weight="%s">%s</text>} \
                $x [expr {$y+$size}] $colour $size $weight [xml $label]]
        }
    }
}

if {$argc != 1} {
    puts stderr "Usage: tclsh snapshot.tcl /path/to/door-game"
    exit 2
}

set commands "[string repeat "step 0 -1\n" 120]quit\n"
if {[catch {
    set result [exec -- [lindex $argv 0] --preview-session << $commands]
    foreach line [split $result "\n"] {
        preview::receive $line
    }
    if {![lindex $preview::frame 6]} {
        error "The scripted game did not reach the goal."
    }
    puts [format {<svg xmlns="http://www.w3.org/2000/svg" width="%s" height="%s" font-family="DejaVu Sans, sans-serif">} \
        $preview::width $preview::height]
    preview::draw svgDraw
    puts {</svg>}
} message]} {
    puts stderr $message
    exit 1
}
