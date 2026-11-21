# Site assets

- `screens/{light,dark}/*.png`: headless renders of the real Ocy app (0.2.x), used inside the "The real app" stage.
- `laptop/laptop-front-{800,1600}.{avif,webp}`: the front-view laptop behind the stage. Front-view laptop image
  supplied by the project owner (AI-generated), cut out for Ocy: the painted checkerboard was removed with a mask
  (largest dark component, holes filled, morphology, 0.8 px feather, edge colour decontaminated) and exported with
  alpha. The display is a flat grey panel; the app screens are composited over it in CSS. The display box on the
  2189 × 1241 cut-out is x 11.192 %, y 5.802 %, w 77.707 %, h 83.642 %.
