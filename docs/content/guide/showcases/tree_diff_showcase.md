[2m════════════════════════════════════════════════════════════════════════════════[0m
[36m[1mTREE DIFF SHOWCASE[0m[39m [2m- Demonstrating Current Limitations[0m
[2m════════════════════════════════════════════════════════════════════════════════[0m

[33m[1mHASH SUPPORT CHECK[0m[39m
[2mChecking which SVG types have vtable.hash filled in:[0m

  String               Hash: [32mYES[39m
  i32                  Hash: [32mYES[39m
  bool                 Hash: [32mYES[39m
  [2m---[0m
  Svg                  Hash: [31mNO[39m
  SvgElement           Hash: [31mNO[39m
  SvgRect              Hash: [31mNO[39m
  SvgCircle            Hash: [31mNO[39m
  SvgGroup             Hash: [31mNO[39m
  Vec<SvgElement>      Hash: [31mNO[39m

[33mConclusion: Custom structs/enums don't have Hash - we need structural hashing![39m

[33m[1mSTRUCTURAL HASHING[0m[39m
[2mBut with Peek::structural_hash, we can hash any Facet type:[0m

  svg1 (red rect)  hash: 098087fb38c04ccd
  svg2 (clone)     hash: 098087fb38c04ccd
  svg3 (blue rect) hash: 9cf9621dc61cc362

  svg1 == svg2: [32mYES[39m (hashes [32mmatch![39m)
  svg1 == svg3: [31mNO[39m (hashes [33mdiffer![39m)

[2mThis showcase demonstrates how facet-diff currently handles tree mutations.[0m
[2mThe goal is to identify areas for improvement with Merkle-tree based diffing.[0m

[2m────────────────────────────────────────────────────────────────────────────────[0m
[33m[1mSCENARIO:[0m[39m [37m[1mDeep Attribute Change[0m[39m
[2mChange a single attribute (fill: red → green) deep in a nested group.[0m
[2m────────────────────────────────────────────────────────────────────────────────[0m

[1mOld diff output:[0m
[38;2;86;95;137m{[39m
  [38;2;86;95;137m.. 2 unchanged fields[39m
  [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
    [1mGroup[0m [38;2;86;95;137m{[39m
      [38;2;86;95;137m.. 1 unchanged field[39m
      [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
        [1mRect[0m [38;2;86;95;137m{[39m
          [38;2;86;95;137m.. 4 unchanged fields[39m
          [38;2;115;218;202mfill[39m[38;2;86;95;137m:[39m [38;2;247;118;142m"red"[39m → [38;2;115;218;202m"green"[39m
        [38;2;86;95;137m}[39m
        
        [1mCircle[0m [38;2;86;95;137m(structurally equal)[39m
        
      [38;2;86;95;137m][39m
    [38;2;86;95;137m}[39m
    
  [38;2;86;95;137m][39m
[38;2;86;95;137m}[39m

[32m[1mNew tree diff (GumTree-style):[0m[39m
  [31mDELETE[39m children.[0].::Group.[0].children.[0].::Rect.[0].fill (b38e5fab)
  [32mINSERT[39m children.[0].::Group.[0].children.[0].::Rect.[0].fill (e194cd46)
  [33mUPDATE[39m children.[0].::Group.[0].children.[0] (13bd5659 → 5737efb9)
  [33mUPDATE[39m  (ffcf95db → ee288114)
  [33mUPDATE[39m children.[0].::Group.[0].children.[0].::Rect.[0] (de6a134b → bfa56914)
  [33mUPDATE[39m children.[0].::Group.[0].children (dd063bac → 0aab8d2c)
  [33mUPDATE[39m children (915fc55e → 4a0e5bd6)
  [33mUPDATE[39m children.[0].::Group.[0] (dc6eb5d4 → 5c7cb257)
  [33mUPDATE[39m children.[0] (c2f87fb5 → 2f09c521)

[2m────────────────────────────────────────────────────────────────────────────────[0m
[33m[1mSCENARIO:[0m[39m [37m[1mSwap Two Children[0m[39m
[2mSwap the order of rect and circle elements. Ideally shows as a reorder, not delete+insert.[0m
[2m────────────────────────────────────────────────────────────────────────────────[0m

[1mOld diff output:[0m
[38;2;86;95;137m{[39m
  [38;2;86;95;137m.. 2 unchanged fields[39m
  [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
    [38;2;247;118;142m- SvgElement::Rect(SvgRect {
      x: "10",
      y: "10",
      width: "30",
      height: "30",
      fill: "red",
    })[39m
    [1mCircle[0m [38;2;86;95;137m(structurally equal)[39m
    
    [38;2;115;218;202m+ SvgElement::Rect(SvgRect {
      x: "10",
      y: "10",
      width: "30",
      height: "30",
      fill: "red",
    })[39m
  [38;2;86;95;137m][39m
[38;2;86;95;137m}[39m

[32m[1mNew tree diff (GumTree-style):[0m[39m
  [36mMOVE[39m children.[0] → children.[1] (4a913cfe)
  [36mMOVE[39m children.[1].::Circle.[0].cy → children.[0].::Circle.[0].cy (ef22003f)
  [36mMOVE[39m children.[1].::Circle.[0].fill → children.[0].::Circle.[0].fill (47976581)
  [36mMOVE[39m children.[0].::Rect.[0].height → children.[1].::Rect.[0].height (c65a86d4)
  [36mMOVE[39m children.[1].::Circle.[0] → children.[0].::Circle.[0] (ad81d4ac)
  [36mMOVE[39m children.[1].::Circle.[0].cx → children.[0].::Circle.[0].cx (ef22003f)
  [36mMOVE[39m children.[0].::Rect.[0].x → children.[1].::Rect.[0].x (b330ed1d)
  [33mUPDATE[39m children (c3749b04 → ff95e3d4)
  [36mMOVE[39m children.[0].::Rect.[0].y → children.[1].::Rect.[0].y (b330ed1d)
  [36mMOVE[39m children.[1] → children.[0] (e9e65ac5)
  [36mMOVE[39m children.[1].::Circle.[0].r → children.[0].::Circle.[0].r (157341be)
  [36mMOVE[39m children.[0].::Rect.[0].width → children.[1].::Rect.[0].width (c65a86d4)
  [36mMOVE[39m children.[0].::Rect.[0].fill → children.[1].::Rect.[0].fill (b38e5fab)
  [36mMOVE[39m children.[0].::Rect.[0] → children.[1].::Rect.[0] (3f785513)
  [33mUPDATE[39m  (c27aab3f → 5e77920c)

[2m────────────────────────────────────────────────────────────────────────────────[0m
[33m[1mSCENARIO:[0m[39m [37m[1mDelete a Child[0m[39m
[2mRemove the middle element (circle) from a list of three elements.[0m
[2m────────────────────────────────────────────────────────────────────────────────[0m

[1mOld diff output:[0m
[38;2;86;95;137m{[39m
  [38;2;86;95;137m.. 2 unchanged fields[39m
  [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
    [38;2;247;118;142m- SvgElement::Rect(SvgRect {
      x: "10",
      y: "10",
      width: "30",
      height: "30",
      fill: "red",
    })[39m
    [38;2;247;118;142m- SvgElement::Circle(SvgCircle {
      cx: "50",
      cy: "50",
      r: "15",
      fill: "green",
    })[39m
    [1mRect[0m [38;2;86;95;137m{[39m
      [38;2;115;218;202mfill[39m[38;2;86;95;137m:[39m [38;2;247;118;142m"blue"[39m → [38;2;115;218;202m"red"[39m
      [38;2;115;218;202mheight[39m[38;2;86;95;137m:[39m [38;2;247;118;142m"20"[39m → [38;2;115;218;202m"30"[39m
      [38;2;115;218;202mwidth[39m[38;2;86;95;137m:[39m [38;2;247;118;142m"20"[39m → [38;2;115;218;202m"30"[39m
      [38;2;115;218;202mx[39m[38;2;86;95;137m:[39m [38;2;247;118;142m"70"[39m → [38;2;115;218;202m"10"[39m
      [38;2;115;218;202my[39m[38;2;86;95;137m:[39m [38;2;247;118;142m"70"[39m → [38;2;115;218;202m"10"[39m
    [38;2;86;95;137m}[39m
    
    [38;2;115;218;202m+ SvgElement::Rect(SvgRect {
      x: "70",
      y: "70",
      width: "20",
      height: "20",
      fill: "blue",
    })[39m
  [38;2;86;95;137m][39m
[38;2;86;95;137m}[39m

[32m[1mNew tree diff (GumTree-style):[0m[39m
  [31mDELETE[39m children.[1] (61589438)
  [31mDELETE[39m children.[1].::Circle.[0] (e416eb17)
  [31mDELETE[39m children.[1].::Circle.[0].cx (7f3ab45d)
  [31mDELETE[39m children.[1].::Circle.[0].cy (7f3ab45d)
  [31mDELETE[39m children.[1].::Circle.[0].r (bd9815b0)
  [31mDELETE[39m children.[1].::Circle.[0].fill (e194cd46)
  [36mMOVE[39m children.[2].::Rect.[0].width → children.[1].::Rect.[0].width (65c61343)
  [36mMOVE[39m children.[2].::Rect.[0].y → children.[1].::Rect.[0].y (ef22003f)
  [36mMOVE[39m children.[2].::Rect.[0].fill → children.[1].::Rect.[0].fill (47976581)
  [36mMOVE[39m children.[2].::Rect.[0] → children.[1].::Rect.[0] (cf74bccb)
  [33mUPDATE[39m children (6145376c → b68c2f81)
  [33mUPDATE[39m  (9626372d → 75bc90b3)
  [36mMOVE[39m children.[2] → children.[1] (7be6dadb)
  [36mMOVE[39m children.[2].::Rect.[0].x → children.[1].::Rect.[0].x (ef22003f)
  [36mMOVE[39m children.[2].::Rect.[0].height → children.[1].::Rect.[0].height (65c61343)

[2m────────────────────────────────────────────────────────────────────────────────[0m
[33m[1mSCENARIO:[0m[39m [37m[1mAdd a Child[0m[39m
[2mInsert a new circle element between two existing rect elements.[0m
[2m────────────────────────────────────────────────────────────────────────────────[0m

[1mOld diff output:[0m
[38;2;86;95;137m{[39m
  [38;2;86;95;137m.. 2 unchanged fields[39m
  [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
    [1mRect[0m [38;2;86;95;137m(structurally equal)[39m
    
    [38;2;115;218;202m+ SvgElement::Circle(SvgCircle {
      cx: "50",
      cy: "50",
      r: "15",
      fill: "green",
    })[39m
    [1mRect[0m [38;2;86;95;137m(structurally equal)[39m
    
  [38;2;86;95;137m][39m
[38;2;86;95;137m}[39m

[32m[1mNew tree diff (GumTree-style):[0m[39m
  [32mINSERT[39m children.[1] (61589438)
  [32mINSERT[39m children.[1].::Circle.[0] (e416eb17)
  [32mINSERT[39m children.[1].::Circle.[0].cx (7f3ab45d)
  [32mINSERT[39m children.[1].::Circle.[0].cy (7f3ab45d)
  [32mINSERT[39m children.[1].::Circle.[0].r (bd9815b0)
  [32mINSERT[39m children.[1].::Circle.[0].fill (e194cd46)
  [36mMOVE[39m children.[1].::Rect.[0] → children.[2].::Rect.[0] (cf74bccb)
  [36mMOVE[39m children.[1].::Rect.[0].y → children.[2].::Rect.[0].y (ef22003f)
  [36mMOVE[39m children.[1].::Rect.[0].height → children.[2].::Rect.[0].height (65c61343)
  [36mMOVE[39m children.[1].::Rect.[0].fill → children.[2].::Rect.[0].fill (47976581)
  [33mUPDATE[39m  (75bc90b3 → 9626372d)
  [36mMOVE[39m children.[1].::Rect.[0].x → children.[2].::Rect.[0].x (ef22003f)
  [36mMOVE[39m children.[1].::Rect.[0].width → children.[2].::Rect.[0].width (65c61343)
  [36mMOVE[39m children.[1] → children.[2] (7be6dadb)
  [33mUPDATE[39m children (b68c2f81 → 6145376c)

[2m────────────────────────────────────────────────────────────────────────────────[0m
[33m[1mSCENARIO:[0m[39m [37m[1mMove a Child Between Groups[0m[39m
[2mMove the circle from the 'left' group to the 'right' group. Ideally detected as a move.[0m
[2m────────────────────────────────────────────────────────────────────────────────[0m

[1mOld diff output:[0m
[38;2;86;95;137m{[39m
  [38;2;86;95;137m.. 2 unchanged fields[39m
  [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
    [1mGroup[0m [38;2;86;95;137m{[39m
      [38;2;86;95;137m.. 1 unchanged field[39m
      [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
        [38;2;247;118;142m- SvgElement::Circle(SvgCircle {
          cx: "25",
          cy: "50",
          r: "20",
          fill: "red",
        })[39m
      [38;2;86;95;137m][39m
    [38;2;86;95;137m}[39m
    
    [1mGroup[0m [38;2;86;95;137m{[39m
      [38;2;86;95;137m.. 1 unchanged field[39m
      [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
        [1mRect[0m [38;2;86;95;137m(structurally equal)[39m
        
        [38;2;115;218;202m+ SvgElement::Circle(SvgCircle {
          cx: "25",
          cy: "50",
          r: "20",
          fill: "red",
        })[39m
      [38;2;86;95;137m][39m
    [38;2;86;95;137m}[39m
    
  [38;2;86;95;137m][39m
[38;2;86;95;137m}[39m

[32m[1mNew tree diff (GumTree-style):[0m[39m
  [31mDELETE[39m children.[1] (c3a21a61)
  [31mDELETE[39m children.[1].::Group.[0] (8e72bc7c)
  [31mDELETE[39m children.[1].::Group.[0].children (dcb0fbb0)
  [32mINSERT[39m children.[0] (a7bbdde3)
  [32mINSERT[39m children.[0].::Group.[0] (cfc183c7)
  [32mINSERT[39m children.[0].::Group.[0].children (e365ff9b)
  [36mMOVE[39m children.[0].::Group.[0].children → children.[1].::Group.[0].children (5497cb6a)
  [36mMOVE[39m children.[0].::Group.[0].children.[0].::Circle.[0].cy → children.[1].::Group.[0].children.[1].::Circle.[0].cy (7f3ab45d)
  [36mMOVE[39m children.[0].::Group.[0].children.[0].::Circle.[0].r → children.[1].::Group.[0].children.[1].::Circle.[0].r (65c61343)
  [36mMOVE[39m children.[0].::Group.[0].children.[0].::Circle.[0] → children.[1].::Group.[0].children.[1].::Circle.[0] (8e0f49a2)
  [36mMOVE[39m children.[0].::Group.[0].children.[0] → children.[1].::Group.[0].children.[1] (3b238eb4)
  [36mMOVE[39m children.[0].::Group.[0].children.[0].::Circle.[0].cx → children.[1].::Group.[0].children.[1].::Circle.[0].cx (157341be)
  [36mMOVE[39m children.[0].::Group.[0] → children.[1].::Group.[0] (100681a8)
  [33mUPDATE[39m children (329b553a → 621ffee4)
  [36mMOVE[39m children.[0].::Group.[0].children.[0].::Circle.[0].fill → children.[1].::Group.[0].children.[1].::Circle.[0].fill (b38e5fab)
  [36mMOVE[39m children.[0] → children.[1] (40215a95)
  [33mUPDATE[39m  (abef1361 → 72b03fd2)

[2m────────────────────────────────────────────────────────────────────────────────[0m
[33m[1mSCENARIO:[0m[39m [37m[1mNested Group Modification[0m[39m
[2mModify circle attributes (fill, r) three levels deep in nested groups.[0m
[2m────────────────────────────────────────────────────────────────────────────────[0m

[1mOld diff output:[0m
[38;2;86;95;137m{[39m
  [38;2;86;95;137m.. 2 unchanged fields[39m
  [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
    [1mGroup[0m [38;2;86;95;137m{[39m
      [38;2;86;95;137m.. 1 unchanged field[39m
      [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
        [1mGroup[0m [38;2;86;95;137m{[39m
          [38;2;86;95;137m.. 1 unchanged field[39m
          [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
            [1mRect[0m [38;2;86;95;137m(structurally equal)[39m
            
          [38;2;86;95;137m][39m
        [38;2;86;95;137m}[39m
        
        [1mGroup[0m [38;2;86;95;137m{[39m
          [38;2;86;95;137m.. 1 unchanged field[39m
          [38;2;115;218;202mchildren[39m[38;2;86;95;137m:[39m [38;2;86;95;137m[[39m
            [1mCircle[0m [38;2;86;95;137m{[39m
              [38;2;86;95;137m.. 2 unchanged fields[39m
              [38;2;115;218;202mfill[39m[38;2;86;95;137m:[39m [38;2;247;118;142m"blue"[39m → [38;2;115;218;202m"yellow"[39m
              [38;2;115;218;202mr[39m[38;2;86;95;137m:[39m [38;2;247;118;142m"30"[39m → [38;2;115;218;202m"40"[39m
            [38;2;86;95;137m}[39m
            
          [38;2;86;95;137m][39m
        [38;2;86;95;137m}[39m
        
      [38;2;86;95;137m][39m
    [38;2;86;95;137m}[39m
    
  [38;2;86;95;137m][39m
[38;2;86;95;137m}[39m

[32m[1mNew tree diff (GumTree-style):[0m[39m
  [31mDELETE[39m children.[0].::Group.[0].children.[1] (8371d7c7)
  [31mDELETE[39m children.[0].::Group.[0].children.[1].::Group.[0] (0538cb6b)
  [31mDELETE[39m children.[0].::Group.[0].children.[1].::Group.[0].children (a117634b)
  [31mDELETE[39m children.[0].::Group.[0].children.[1].::Group.[0].children.[0] (862bfc24)
  [31mDELETE[39m children.[0].::Group.[0].children.[1].::Group.[0].children.[0].::Circle.[0] (5d473db5)
  [31mDELETE[39m children.[0].::Group.[0].children.[1].::Group.[0].children.[0].::Circle.[0].r (c65a86d4)
  [31mDELETE[39m children.[0].::Group.[0].children.[1].::Group.[0].children.[0].::Circle.[0].fill (47976581)
  [32mINSERT[39m children.[0].::Group.[0].children.[1] (331a3174)
  [32mINSERT[39m children.[0].::Group.[0].children.[1].::Group.[0] (539f64f2)
  [32mINSERT[39m children.[0].::Group.[0].children.[1].::Group.[0].children (45e51bd8)
  [32mINSERT[39m children.[0].::Group.[0].children.[1].::Group.[0].children.[0] (02f36a3e)
  [32mINSERT[39m children.[0].::Group.[0].children.[1].::Group.[0].children.[0].::Circle.[0] (ea7fc585)
  [32mINSERT[39m children.[0].::Group.[0].children.[1].::Group.[0].children.[0].::Circle.[0].r (91479cb5)
  [32mINSERT[39m children.[0].::Group.[0].children.[1].::Group.[0].children.[0].::Circle.[0].fill (8e217af1)
  [33mUPDATE[39m children.[0] (b231b18e → 15d940c6)
  [33mUPDATE[39m children.[0].::Group.[0] (84e3e19f → 00eabc71)
  [33mUPDATE[39m children (da5db4a9 → a49ae371)
  [33mUPDATE[39m  (f70e7cb3 → ab967e5f)
  [33mUPDATE[39m children.[0].::Group.[0].children (14d56a79 → 5902ba00)

[2m════════════════════════════════════════════════════════════════════════════════[0m
[36m[1mEND OF SHOWCASE[0m[39m
[2m════════════════════════════════════════════════════════════════════════════════[0m
