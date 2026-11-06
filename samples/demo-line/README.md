# Demo line — a finished AryaCADElectrical sample project

A small machine line drawn with the electrical layer, so the outputs can be inspected
without drawing anything first. Open `project.eprj` (or `sheet1.dxf` directly) and run the
commands from `docs/QUICKSTART.md`.

## What is in here

| file | what it shows |
|---|---|
| `sheet1.dxf` | one schematic sheet: a DOL starter (`ELECSYSTEM INSERT DOL_STARTER`) and a reversing starter (at 0,-120), five wire runs with wire numbers, tags -K1..-K5, -M1..-M2, -S1..-S3, and a drawn shielded 3-core cable `W1` (Motor feeder, `LIYCY-3X1.5`) |
| `panel.dxf` | the generated panel layout for all ten devices: 420×380 mm enclosure, one 35 mm DIN rail, three door cut-outs (two push-buttons + the DOL start button), footprints from `library/panel/footprints.tsv` |
| `cables.tsv` | the cable schedule record with the drawn route and `SHIELD=yes` |
| `project.eprj` | two sheets, plus the option code `option -K1 pump-set-B` |
| `variant-pumpset_b.eprj` | an alternative sheet set copied from the master (`ELECVARIANT NEW pumpset_b`) |

## Commands worth trying on it

```
ELECLIST
ELECWIRELIST wirelist.csv
ELECCABLELIST cables.csv
ELECBOM bom.csv
ELECREPORT report.xlsx
ELECPANEL panel2.dxf
ELECVARIANT LIST
ELECOPTION LIST
ELECHELP
```

## How it was made

The project was generated headlessly (see `reports/PROGRESS.md`, round 13) with:

```
ELECPROJNEW samples\demo-line
ELECNEW
ELECLIB library\starter
ELECSYSTEM INSERT DOL_STARTER
ELECSYSTEM INSERT reversing_starter 0,-120
ELECCABLEDRAW W1 CORES=3 SHIELD=yes PART=LIYCY-3X1.5 DESC=Motor feeder   (points 0.5,0 / 20,40 / 0.5,-120)
ELECWIRENUMBER
ELECPANEL samples\demo-line\panel.dxf
ELECVARIANT NEW pumpset_b
ELECOPTION -K1 pump-set-B
QSAVE
```

The drawings are ordinary DXFs: reopen one, run `ELECLIST`, and the devices are still there.
